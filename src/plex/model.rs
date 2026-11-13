use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Pin {
    pub id: u64,
    pub code: String,
    #[serde(rename = "authToken")]
    pub auth_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Connection {
    pub uri: String,
    #[serde(default)]
    pub local: bool,
    #[serde(default)]
    pub relay: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Resource {
    pub name: String,
    #[serde(default)]
    pub provides: String,
    #[serde(rename = "accessToken")]
    pub access_token: Option<String>,
    #[serde(default)]
    pub connections: Vec<Connection>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Server {
    pub name: String,
    pub uri: String,
    pub token: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Section {
    pub key: String,
    pub title: String,
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Part {
    pub key: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Media {
    #[serde(rename = "Part", default)]
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Movie,
    Show,
    Season,
    Episode,
    Other,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Item {
    #[serde(rename = "ratingKey")]
    pub rating_key: String,
    pub key: String,
    #[serde(rename = "type")]
    pub kind_name: String,
    pub title: String,
    #[serde(rename = "parentTitle")]
    pub parent_title: Option<String>,
    #[serde(rename = "grandparentTitle")]
    pub grandparent_title: Option<String>,
    #[serde(rename = "parentRatingKey")]
    pub parent_rating_key: Option<String>,
    pub index: Option<u32>,
    #[serde(rename = "parentIndex")]
    pub parent_index: Option<u32>,
    pub year: Option<u32>,
    #[serde(rename = "viewOffset")]
    pub view_offset: Option<u64>,
    #[serde(rename = "viewCount")]
    pub view_count: Option<u32>,
    pub duration: Option<u64>,
    #[serde(rename = "leafCount")]
    pub leaf_count: Option<u32>,
    #[serde(rename = "viewedLeafCount")]
    pub viewed_leaf_count: Option<u32>,
    #[serde(rename = "Media", default)]
    pub media: Vec<Media>,
}

impl Item {
    pub fn kind(&self) -> ItemKind {
        match self.kind_name.as_str() {
            "movie" => ItemKind::Movie,
            "show" => ItemKind::Show,
            "season" => ItemKind::Season,
            "episode" => ItemKind::Episode,
            _ => ItemKind::Other,
        }
    }

    pub fn playable(&self) -> bool {
        matches!(self.kind(), ItemKind::Movie | ItemKind::Episode)
    }

    pub fn part_key(&self) -> Option<&str> {
        self.media.first().and_then(|media| media.parts.first()).map(|part| part.key.as_str())
    }

    pub fn watched(&self) -> bool {
        match self.kind() {
            ItemKind::Show | ItemKind::Season => self.leaf_count.is_some() && self.leaf_count == self.viewed_leaf_count,
            _ => self.view_count.unwrap_or(0) > 0,
        }
    }

    pub fn resume_seconds(&self) -> f64 {
        self.view_offset.unwrap_or(0) as f64 / 1000.0
    }

    pub fn label(&self) -> String {
        match self.kind() {
            ItemKind::Episode => episode_label(self),
            ItemKind::Movie => movie_label(self),
            _ => self.title.clone(),
        }
    }

    pub fn heading(&self) -> String {
        match (self.kind(), &self.grandparent_title) {
            (ItemKind::Episode, Some(show)) => format!("{show} · {}", self.label()),
            _ => self.label(),
        }
    }
}

fn episode_label(item: &Item) -> String {
    match (item.parent_index, item.index) {
        (Some(season), Some(episode)) => format!("S{season:02}E{episode:02} · {}", item.title),
        (None, Some(episode)) => format!("{episode}. {}", item.title),
        _ => item.title.clone(),
    }
}

fn movie_label(item: &Item) -> String {
    item.year.map(|year| format!("{} ({year})", item.title)).unwrap_or_else(|| item.title.clone())
}

#[derive(Debug, Deserialize)]
pub struct Envelope<T> {
    #[serde(rename = "MediaContainer")]
    pub container: T,
}

#[derive(Debug, Deserialize)]
pub struct Sections {
    #[serde(rename = "Directory", default)]
    pub sections: Vec<Section>,
}

#[derive(Debug, Deserialize)]
pub struct Items {
    #[serde(rename = "Metadata", default)]
    pub items: Vec<Item>,
}

#[derive(Debug, Deserialize)]
pub struct Hub {
    #[serde(rename = "Metadata", default)]
    pub items: Vec<Item>,
}

#[derive(Debug, Deserialize)]
pub struct Hubs {
    #[serde(rename = "Hub", default)]
    pub hubs: Vec<Hub>,
}

fn plain(text: &str) -> String {
    text.to_lowercase()
        .split(|letter: char| !letter.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_year(word: &str) -> bool {
    word.len() == 4 && word.chars().all(|letter| letter.is_ascii_digit())
}

pub fn without_year(words: &str) -> String {
    let all: Vec<&str> = words.split_whitespace().collect();
    match all.split_last() {
        Some((last, title)) if is_year(last) && !title.is_empty() => title.join(" "),
        _ => all.join(" "),
    }
}

fn names(item: &Item) -> [String; 2] {
    let with_year = item.year.map(|year| format!("{} {year}", item.title)).unwrap_or_default();
    [plain(&item.title), plain(&with_year)]
}

fn closeness(item: &Item, wanted: &str) -> u8 {
    let names = names(item);
    if names.iter().any(|name| name == wanted) {
        return 0;
    }
    if names.iter().any(|name| name.starts_with(wanted)) {
        return 1;
    }
    2
}

pub fn best_match<'a>(items: &'a [Item], words: &str) -> Option<&'a Item> {
    let wanted = plain(words);
    items
        .iter()
        .filter(|item| item.playable())
        .min_by_key(|item| (closeness(item, &wanted), item.kind() != ItemKind::Movie))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titled(kind: &str, title: &str, year: u32) -> Item {
        serde_json::from_str(&format!(
            r#"{{"ratingKey":"{title}","key":"/k","type":"{kind}","title":"{title}","year":{year}}}"#
        ))
        .unwrap()
    }

    #[test]
    fn asking_for_a_film_by_name_beats_an_episode_that_only_mentions_it() {
        let found = [
            titled("episode", "American Poison", 2023),
            titled("show", "Poison", 2020),
            titled("movie", "Poison", 2023),
        ];
        assert_eq!(best_match(&found, "poison").map(|item| item.kind()), Some(ItemKind::Movie));
        assert_eq!(best_match(&found, "Poison 2023").map(|item| item.kind()), Some(ItemKind::Movie));
        assert_eq!(best_match(&found, "american").map(|item| item.title.as_str()), Some("American Poison"));
    }

    #[test]
    fn a_trailing_year_is_left_out_of_the_search_but_a_title_that_is_a_year_is_kept() {
        assert_eq!(without_year("poison 2023"), "poison");
        assert_eq!(without_year("2012"), "2012");
        assert_eq!(without_year("blade runner 2049"), "blade runner");
    }

    #[test]
    fn nothing_playable_means_no_match() {
        assert_eq!(best_match(&[titled("show", "Poison", 2020)], "poison"), None);
    }

    fn episode() -> Item {
        serde_json::from_str(
            r#"{"ratingKey":"12","key":"/library/metadata/12","type":"episode","title":"Pilot",
            "grandparentTitle":"Lost","parentIndex":1,"index":2,"viewOffset":61500,
            "Media":[{"Part":[{"key":"/library/parts/9/file.mkv"}]}]}"#,
        )
        .unwrap()
    }

    #[test]
    fn episode_reads_as_show_season_and_number() {
        assert_eq!(episode().heading(), "Lost · S01E02 · Pilot");
    }

    #[test]
    fn episode_resumes_where_plex_left_it() {
        assert_eq!(episode().resume_seconds(), 61.5);
    }

    #[test]
    fn episode_plays_its_first_part() {
        assert_eq!(episode().part_key(), Some("/library/parts/9/file.mkv"));
    }

    #[test]
    fn show_is_watched_only_when_every_episode_is() {
        let show = |viewed: u32| -> Item {
            serde_json::from_str(&format!(
                r#"{{"ratingKey":"1","key":"/library/metadata/1/children","type":"show",
                "title":"Lost","leafCount":3,"viewedLeafCount":{viewed}}}"#
            ))
            .unwrap()
        };
        assert!(show(3).watched());
        assert!(!show(2).watched());
    }
}
