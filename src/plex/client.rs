use std::fmt;
use std::time::Duration;

use serde::de::DeserializeOwned;
use ureq::Agent;
use ureq::http::Response;

use super::model::{Envelope, Hubs, Item, Items, Pin, Resource, Section, Sections, Server};

const PRODUCT: &str = "sth-plex-pip";
const PLEX_TV: &str = "https://plex.tv/api/v2";

#[derive(Debug)]
pub enum PlexError {
    Unauthorized,
    Unreachable(String),
    Unexpected(String),
    NoServer,
}

impl fmt::Display for PlexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unauthorized => write!(f, "Plex no longer accepts this sign-in. Sign in again."),
            Self::Unreachable(detail) => write!(f, "Could not reach Plex: {detail}"),
            Self::Unexpected(detail) => write!(f, "Plex answered with something unexpected: {detail}"),
            Self::NoServer => write!(f, "No reachable Plex Media Server on this account."),
        }
    }
}

impl From<ureq::Error> for PlexError {
    fn from(error: ureq::Error) -> Self {
        match error {
            ureq::Error::StatusCode(401) => Self::Unauthorized,
            ureq::Error::StatusCode(code) => Self::Unexpected(format!("HTTP {code}")),
            ureq::Error::Json(detail) => Self::Unexpected(detail.to_string()),
            other => Self::Unreachable(other.to_string()),
        }
    }
}

pub type PlexResult<T> = Result<T, PlexError>;

#[derive(Debug, Clone, PartialEq)]
pub struct Identity {
    pub client_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayState {
    Playing,
    Paused,
    Stopped,
}

impl PlayState {
    fn name(self) -> &'static str {
        match self {
            Self::Playing => "playing",
            Self::Paused => "paused",
            Self::Stopped => "stopped",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    pub rating_key: String,
    pub key: String,
    pub state: PlayState,
    pub position_ms: u64,
    pub duration_ms: u64,
}

fn agent(timeout: Duration) -> Agent {
    Agent::config_builder()
        .timeout_global(Some(timeout))
        .build()
        .into()
}

fn device_name() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| PRODUCT.to_string())
}

fn identified<B>(
    request: ureq::RequestBuilder<B>,
    identity: &Identity,
    token: Option<&str>,
) -> ureq::RequestBuilder<B> {
    let request = request
        .header("Accept", "application/json")
        .header("X-Plex-Product", PRODUCT)
        .header("X-Plex-Version", env!("CARGO_PKG_VERSION"))
        .header("X-Plex-Platform", std::env::consts::OS)
        .header("X-Plex-Device", std::env::consts::OS)
        .header("X-Plex-Device-Name", device_name())
        .header("X-Plex-Client-Identifier", &identity.client_id);
    match token {
        Some(token) => request.header("X-Plex-Token", token),
        None => request,
    }
}

fn json<T: DeserializeOwned>(response: Result<Response<ureq::Body>, ureq::Error>) -> PlexResult<T> {
    Ok(response?.body_mut().read_json::<T>()?)
}

pub fn request_pin(identity: &Identity) -> PlexResult<Pin> {
    let request = agent(Duration::from_secs(15)).post(format!("{PLEX_TV}/pins?strong=true"));
    json(identified(request, identity, None).send_empty())
}

pub fn sign_in_url(identity: &Identity, pin: &Pin) -> String {
    format!(
        "https://app.plex.tv/auth#?clientID={}&code={}&context%5Bdevice%5D%5Bproduct%5D={PRODUCT}",
        identity.client_id, pin.code
    )
}

pub fn claimed_token(identity: &Identity, pin: &Pin) -> PlexResult<Option<String>> {
    let request = agent(Duration::from_secs(15)).get(format!("{PLEX_TV}/pins/{}", pin.id));
    let pin: Pin = json(identified(request, identity, None).call())?;
    Ok(pin.auth_token.filter(|token| !token.is_empty()))
}

fn resources(identity: &Identity, token: &str) -> PlexResult<Vec<Resource>> {
    let request = agent(Duration::from_secs(20))
        .get(format!("{PLEX_TV}/resources?includeHttps=1&includeRelay=1"));
    json(identified(request, identity, Some(token)).call())
}

fn connection_rank(connection: &super::model::Connection) -> u8 {
    match (connection.local, connection.relay) {
        (true, _) => 0,
        (false, false) => 1,
        (false, true) => 2,
    }
}

fn answers(identity: &Identity, uri: &str, token: &str) -> bool {
    let request = agent(Duration::from_secs(4)).get(format!("{uri}/identity"));
    identified(request, identity, Some(token)).call().is_ok()
}

fn reachable(identity: &Identity, resource: &Resource) -> Option<Server> {
    let token = resource.access_token.clone()?;
    let ranked = {
        let mut connections = resource.connections.clone();
        connections.sort_by_key(connection_rank);
        connections
    };
    ranked
        .iter()
        .find(|connection| answers(identity, &connection.uri, &token))
        .map(|connection| Server {
            name: resource.name.clone(),
            uri: connection.uri.clone(),
            token,
        })
}

pub fn servers(identity: &Identity, token: &str) -> PlexResult<Vec<Server>> {
    let found: Vec<Server> = resources(identity, token)?
        .iter()
        .filter(|resource| resource.provides.split(',').any(|role| role == "server"))
        .filter_map(|resource| reachable(identity, resource))
        .collect();
    if found.is_empty() {
        return Err(PlexError::NoServer);
    }
    Ok(found)
}

#[derive(Debug, Clone)]
pub struct Library {
    identity: Identity,
    server: Server,
    agent: Agent,
}

impl Library {
    pub fn new(identity: Identity, server: Server) -> Self {
        Self {
            identity,
            server,
            agent: agent(Duration::from_secs(30)),
        }
    }

    pub fn server(&self) -> &Server {
        &self.server
    }

    pub fn stream_url(&self, part_key: &str) -> String {
        format!("{}{part_key}", self.server.uri)
    }

    fn get(&self, path: &str) -> ureq::RequestBuilder<ureq::typestate::WithoutBody> {
        let request = self.agent.get(format!("{}{path}", self.server.uri));
        identified(request, &self.identity, Some(&self.server.token))
    }

    fn items(&self, request: ureq::RequestBuilder<ureq::typestate::WithoutBody>) -> PlexResult<Vec<Item>> {
        json::<Envelope<Items>>(request.call()).map(|envelope| envelope.container.items)
    }

    pub fn sections(&self) -> PlexResult<Vec<Section>> {
        json::<Envelope<Sections>>(self.get("/library/sections").call())
            .map(|envelope| envelope.container.sections)
            .map(|sections| {
                sections
                    .into_iter()
                    .filter(|section| matches!(section.kind.as_str(), "movie" | "show"))
                    .collect()
            })
    }

    pub fn section_items(&self, section_key: &str) -> PlexResult<Vec<Item>> {
        self.items(self.get(&format!("/library/sections/{section_key}/all")))
    }

    pub fn children(&self, rating_key: &str) -> PlexResult<Vec<Item>> {
        self.items(self.get(&format!("/library/metadata/{rating_key}/children")))
    }

    pub fn item(&self, rating_key: &str) -> PlexResult<Option<Item>> {
        self.items(self.get(&format!("/library/metadata/{rating_key}")))
            .map(|items| items.into_iter().next())
    }

    pub fn on_deck(&self) -> PlexResult<Vec<Item>> {
        self.items(self.get("/library/onDeck"))
    }

    pub fn search(&self, words: &str) -> PlexResult<Vec<Item>> {
        let request = self.get("/hubs/search").query("query", words).query("limit", "30");
        json::<Envelope<Hubs>>(request.call()).map(|envelope| {
            envelope
                .container
                .hubs
                .into_iter()
                .flat_map(|hub| hub.items)
                .filter(|item| !matches!(item.kind(), super::model::ItemKind::Other))
                .collect()
        })
    }

    pub fn report(&self, progress: &Progress) -> PlexResult<()> {
        self.get("/:/timeline")
            .query("ratingKey", &progress.rating_key)
            .query("key", &progress.key)
            .query("state", progress.state.name())
            .query("time", progress.position_ms.to_string())
            .query("duration", progress.duration_ms.to_string())
            .call()
            .map(|_| ())
            .map_err(PlexError::from)
    }

    pub fn mark_watched(&self, rating_key: &str) -> PlexResult<()> {
        self.get("/:/scrobble")
            .query("identifier", "com.plexapp.plugins.library")
            .query("key", rating_key)
            .call()
            .map(|_| ())
            .map_err(PlexError::from)
    }
}
