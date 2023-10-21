use std::collections::HashMap;
use thiserror::Error;
use url::Url;

/// `Error` represents an error that can occur while using the MSUC client.
#[derive(Error, Debug)]
pub enum Error {
    #[error("request error: {0}")]
    Client(#[from] reqwest::Error),
    #[error("parsing error: {0}")]
    Parsing(String),
    #[error("search error: {0}")]
    Search(String),
    #[error("internal error: {0}")]
    Internal(String),
    #[error("Microsoft Update Catalog error: {0}, code: {1}")]
    Msuc(String, String),
}

/// `SearchPage` represents a page of search results and the metadata needed to retrieve the next.
pub type SearchPage = (SearchPageMeta, Vec<SearchResult>);

/// `SearchResult` represents a single update search result from the Microsoft Update Catalog.
#[derive(Eq, PartialEq, Debug)]
pub struct SearchResult {
    pub title: String,
    pub id: String,
    pub kb: String,
    pub product: String,
    pub classification: String,
    pub last_modified: chrono::NaiveDate,
    pub version: Option<String>,
    pub size: u64,
}

/// `SearchPageMeta` is an internal state tracker for a SearchResultStream page.
#[derive(Eq, PartialEq, Debug)]
pub struct SearchPageMeta {
    pub event_target: String,
    pub event_argument: String,
    pub event_validation: String,
    pub view_state: String,
    pub view_state_generator: String,
    pub pagination: SearchPagePaginationMeta,
}

impl SearchPageMeta {
    /// `as_map` returns a HashMap of the metadata values, excluding the pagination metadata.
    pub fn as_map(&self) -> HashMap<&str, &str> {
        let mut map = HashMap::new();
        map.insert("__EVENTTARGET", self.event_target.as_str());
        map.insert("__EVENTARGUMENT", self.event_argument.as_str());
        map.insert("__EVENTVALIDATION", self.event_validation.as_str());
        map.insert("__VIEWSTATE", self.view_state.as_str());
        map.insert("__VIEWSTATEGENERATOR", self.view_state_generator.as_str());

        map
    }
}

/// `SearchPagePaginationMeta` contains page count information for a SearchResultStream page.
#[derive(Eq, PartialEq, Debug)]
pub struct SearchPagePaginationMeta {
    pub has_next_page: bool,
    pub too_many_results: bool,
    pub current_page: i16,
    pub page_size: i16,
    pub page_count: i16,
    pub result_count: i16,
}

impl Default for SearchPagePaginationMeta {
    /// `default` creates a new SearchPagePaginationMeta with all values set to 0
    fn default() -> Self {
        SearchPagePaginationMeta {
            // has_next_page is set to true for the first page
            has_next_page: true,
            too_many_results: false,
            current_page: 0,
            page_size: 0,
            page_count: 0,
            result_count: 0,
        }
    }
}

impl Default for SearchPageMeta {
    /// `default` creates a new SearchPage with empty values and the `has_next_page` set to true
    /// for the first page
    fn default() -> Self {
        SearchPageMeta {
            event_target: "".to_string(),
            event_argument: "".to_string(),
            event_validation: "".to_string(),
            view_state: "".to_string(),
            view_state_generator: "".to_string(),
            pagination: SearchPagePaginationMeta::default(),
        }
    }
}

/// `Update` represents the details of a single update from the Microsoft Update Catalog.
#[derive(Eq, PartialEq, Debug)]
pub struct Update {
    pub title: String,
    pub id: String,
    pub kb: String,
    pub classification: String,
    pub last_modified: chrono::NaiveDate,
    pub size: u64,
    pub description: String,
    pub architecture: Option<String>,
    pub supported_products: Vec<String>,
    pub supported_languages: Vec<String>,
    pub msrc_number: Option<String>,
    pub msrc_severity: Option<String>,
    pub info_url: Url,
    pub support_url: Url,
    pub reboot_behavior: RebootBehavior,
    pub requires_user_input: bool,
    pub is_exclusive_install: bool,
    pub requires_network_connectivity: bool,
    pub uninstall_notes: Option<String>,
    pub uninstall_steps: Option<String>,
    pub supersedes: Vec<SupersedesUpdate>,
    pub superseded_by: Vec<SupersededByUpdate>,
}

/// `SupersededByUpdate` represents an update that supersedes the current update.
#[derive(Eq, PartialEq, Debug)]
pub struct SupersededByUpdate {
    pub title: String,
    pub kb: String,
    pub id: String,
}

/// `SupersedesUpdate` represents an update that the current update supersedes.
#[derive(Eq, PartialEq, Debug)]
pub struct SupersedesUpdate {
    pub title: String,
    pub kb: String,
}

/// `RebootBehavior` represents the reboot behavior of an update.
#[derive(Eq, PartialEq, Debug)]
pub enum RebootBehavior {
    Required,
    CanRequest,
    Recommended,
    NotRequired,
    NeverRestarts,
}