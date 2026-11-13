use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::Duration;

use super::client::{self, Identity, Library, PlexError, PlexResult, Progress};
use super::model::{Item, Section};

const SIGN_IN_POLLS: u32 = 150;
const SIGN_IN_POLL_GAP: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq)]
pub enum Listing {
    OnDeck,
    Section(Section),
    Children(Box<Item>),
    Search(String),
}

#[derive(Debug)]
pub enum Request {
    SignIn,
    Connect { token: String },
    Sections,
    List(Listing),
    Following { episode: Box<Item> },
    Report(Progress),
    MarkWatched { rating_key: String },
}

#[derive(Debug)]
pub enum Reply {
    SignInOpened { url: String },
    SignedIn { token: String },
    Connected { library: Library },
    Sections(Vec<Section>),
    Listed { listing: Listing, items: Vec<Item> },
    Following { rating_key: String, items: Vec<Item> },
    SignedOut,
    Failed(String),
}

pub type Wake = Arc<dyn Fn() + Send + Sync>;

pub struct PlexWorker {
    requests: Sender<Request>,
    replies: Receiver<Reply>,
}

impl PlexWorker {
    pub fn start(identity: Identity, wake: Wake) -> Self {
        let (requests, inbox) = channel::<Request>();
        let (outbox, replies) = channel::<Reply>();
        thread::spawn(move || serve(identity, inbox, outbox, wake));
        Self { requests, replies }
    }

    pub fn ask(&self, request: Request) {
        let _ = self.requests.send(request);
    }

    pub fn replies(&self) -> Vec<Reply> {
        self.replies.try_iter().collect()
    }
}

fn serve(identity: Identity, inbox: Receiver<Request>, outbox: Sender<Reply>, wake: Wake) {
    let say = |reply: Reply| {
        let _ = outbox.send(reply);
        wake();
    };
    inbox.iter().fold(None::<Library>, |library, request| {
        let (library, replies) = answer(&identity, library, request, &say);
        replies.into_iter().for_each(&say);
        library
    });
}

fn answer(
    identity: &Identity,
    library: Option<Library>,
    request: Request,
    say: &dyn Fn(Reply),
) -> (Option<Library>, Vec<Reply>) {
    match request {
        Request::SignIn => (library, vec![settled(sign_in(identity, say))]),
        Request::Connect { token } => connect(identity, &token),
        other => {
            let replies = library.as_ref().map(|library| browse(library, other)).unwrap_or_default();
            (library, replies)
        }
    }
}

fn settled(result: PlexResult<Reply>) -> Reply {
    match result {
        Ok(reply) => reply,
        Err(PlexError::Unauthorized) => Reply::SignedOut,
        Err(error) => Reply::Failed(error.to_string()),
    }
}

fn sign_in(identity: &Identity, say: &dyn Fn(Reply)) -> PlexResult<Reply> {
    let pin = client::request_pin(identity)?;
    let url = client::sign_in_url(identity, &pin);
    open_in_browser(&url);
    say(Reply::SignInOpened { url });
    (0..SIGN_IN_POLLS)
        .find_map(|_| {
            thread::sleep(SIGN_IN_POLL_GAP);
            client::claimed_token(identity, &pin).transpose()
        })
        .unwrap_or_else(|| Err(PlexError::Unreachable("the sign-in was not finished in time".into())))
        .map(|token| Reply::SignedIn { token })
}

fn connect(identity: &Identity, token: &str) -> (Option<Library>, Vec<Reply>) {
    let library = client::servers(identity, token)
        .map(|servers| servers.into_iter().next())
        .map(|server| server.map(|server| Library::new(identity.clone(), server)));
    match library {
        Ok(Some(library)) => (Some(library.clone()), vec![Reply::Connected { library }]),
        Ok(None) => (None, vec![Reply::Failed(PlexError::NoServer.to_string())]),
        Err(error) => (None, vec![settled(Err(error))]),
    }
}

fn browse(library: &Library, request: Request) -> Vec<Reply> {
    match request {
        Request::Sections => vec![settled(library.sections().map(Reply::Sections))],
        Request::List(listing) => vec![settled(list(library, &listing).map(|items| Reply::Listed { listing, items }))],
        Request::Following { episode } => following(library, &episode).into_iter().collect(),
        Request::Report(progress) => failures(library.report(&progress)),
        Request::MarkWatched { rating_key } => failures(library.mark_watched(&rating_key)),
        Request::SignIn | Request::Connect { .. } => vec![],
    }
}

fn following(library: &Library, episode: &Item) -> Option<Reply> {
    let season = episode.parent_rating_key.as_deref()?;
    let items = library.children(season).ok()?.into_iter().filter(|other| other.index > episode.index).collect();
    Some(Reply::Following { rating_key: episode.rating_key.clone(), items })
}

fn failures(result: PlexResult<()>) -> Vec<Reply> {
    match result {
        Err(PlexError::Unauthorized) => vec![Reply::SignedOut],
        _ => vec![],
    }
}

fn list(library: &Library, listing: &Listing) -> PlexResult<Vec<Item>> {
    match listing {
        Listing::OnDeck => library.on_deck(),
        Listing::Section(section) => library.section_items(&section.key),
        Listing::Children(item) => library.children(&item.rating_key),
        Listing::Search(words) => library.search(words),
    }
}

fn open_in_browser(url: &str) {
    let (program, arguments): (&str, Vec<&str>) = match std::env::consts::OS {
        "macos" => ("open", vec![url]),
        "windows" => ("rundll32", vec!["url.dll,FileProtocolHandler", url]),
        _ => ("xdg-open", vec![url]),
    };
    let _ = std::process::Command::new(program).args(arguments).spawn();
}
