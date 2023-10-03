use std::collections::HashMap;
use std::num::ParseIntError;
#[cfg(not(feature = "blocking"))]
use reqwest::RequestBuilder;
#[cfg(feature = "blocking")]
use reqwest::blocking::RequestBuilder;
use scraper::{Html, Selector};
use thiserror::Error;
use url::Url;


const LIB_VERSION: &str = env!("CARGO_PKG_VERSION");

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

#[derive(Eq, PartialEq, Debug)]
struct SearchPageMeta {
    event_target: String,
    event_argument: String,
    event_validation: String,
    view_state: String,
    view_state_generator: String,
    has_next_page: bool,
    // TODO: surface this to the user
    too_many_results: bool,
    // TODO: add page count parsing
}

impl SearchPageMeta {
    fn as_map(&self) -> HashMap<&str, &str> {
        let mut map = HashMap::new();
        map.insert("__EVENTTARGET", self.event_target.as_str());
        map.insert("__EVENTARGUMENT", self.event_argument.as_str());
        map.insert("__EVENTVALIDATION", self.event_validation.as_str());
        map.insert("__VIEWSTATE", self.view_state.as_str());
        map.insert("__VIEWSTATEGENERATOR", self.view_state_generator.as_str());

        map
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
            has_next_page: true,
            too_many_results: false,
        }
    }
}

type SearchPage = (SearchPageMeta, SearchResults);

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

#[derive(Eq, PartialEq, Debug)]
pub struct SupersededByUpdate {
    pub title: String,
    pub kb: String,
    pub id: String,
}

#[derive(Eq, PartialEq, Debug)]
pub struct SupersedesUpdate {
    pub title: String,
    pub kb: String,
}

#[derive(Eq, PartialEq, Debug)]
pub enum RebootBehavior {
    Required,
    CanRequest,
    Recommended,
    NotRequired,
    NeverRestarts,
}

/// SearchResults represents a collection of updates returned from a search.
pub type SearchResults = Vec<SearchResult>;

/// `Client` represents a client for the Microsoft Update Catalog.
pub struct Client {
    #[cfg(feature = "blocking")]
    client: reqwest::blocking::Client,
    #[cfg(not(feature = "blocking"))]
    client: reqwest::Client,
    search_url: String,
    update_url: String,
}

impl Default for Client {
    /// `default` creates a new MSUC `Client` with default values. It will panic if
    /// there is an error creating the client. The `new` method should be used instead which allows
    /// for handling the error.
    fn default() -> Client {
        Client::new().expect("Failed to create default client")
    }
}

impl Client {
    /// `new` creates a new MSUC `Client` with default values.
    /// The client does not support non-async operation at this time.
    ///
    /// # Example
    ///
    /// ```
    /// use msuc::Client as MsucClient;
    /// let msuc_client = MsucClient::new().expect("Failed to create MSUC client");
    /// ```
    pub fn new() -> Result<Self, Error> {
        #[cfg(not(feature = "blocking"))]
            let client = reqwest::Client::builder()
            .user_agent(format!("msuc-rs/{}", LIB_VERSION))
            .build()
            .map_err(Error::ClientError)?;
        #[cfg(feature = "blocking")]
            let client = reqwest::blocking::Client::builder()
            .user_agent(format!("msuc-rs/{}", LIB_VERSION))
            .build()
            .map_err(Error::ClientError)?;

        Ok(Client {
            client,
            search_url: String::from("https://www.catalog.update.microsoft.com/Search.aspx"),
            update_url: String::from("https://www.catalog.update.microsoft.com/ScopedViewInline.aspx?updateid="),
        })
    }

    fn get_search_builder(
        &self,
        query: &str,
        meta: &SearchPageMeta,
    ) -> Result<RequestBuilder, Error> {
        let mut u = Url::parse(&self.search_url).map_err(|e|
            Error::InternalError(format!("Failed to parse search url '{}': {}", self.search_url, e.to_string()))
        )?;
        u.set_query(Some(&format!("q={}", query)));
        match meta.event_target.as_str() {
            "" => Ok(self.client
                .get(u.as_str())),
            _ => {
                Ok(self.client.post(u.as_str())
                       .form(&meta.as_map()),
                )
            }
        }
    }

    /// `search` performs a search against the Microsoft Update Catalog, paginating
    /// through all results.
    ///
    /// # Parameters
    ///
    /// * `query` - The query string for the search
    ///
    /// # Example
    ///
    /// ```
    /// use msuc::Client as MsucClient;
    /// use tokio_test;
    ///
    /// tokio_test::block_on(async {
    ///     let msuc_client = MsucClient::new().expect("Failed to create MSUC client");
    ///     let resp = msuc_client.search("MS08-067").await.expect("Failed to search");
    ///     for update in resp.unwrap().iter() {
    ///       println!("Found update: {}", update.title);
    ///     }
    /// });
    #[cfg(not(feature = "blocking"))]
    pub async fn search(&self, query: &str) -> Result<Option<SearchResults>, Error> {
        let mut results: SearchResults = vec![];
        let mut meta = SearchPageMeta::default();

        loop {
            let builder = self.get_search_builder(
                query,
                &meta,
            )?;
            let resp = builder.send().await.map_err(Error::ClientError)?;

            resp.error_for_status_ref()?;
            let html = resp.text().await.map_err(Error::ClientError)?;
            let res_page = parse_search_results(&html
            ).map_err(|e| Error::SearchError(
                format!("Failed to parse search results for {}: {:?}", query, e)
            ))?;

            match res_page {
                Some(p) => {
                    meta = p.0;
                    results.extend(p.1);
                    if !meta.has_next_page {
                        break;
                    }
                }
                None => {
                    break;
                }
            }
        }

        if results.is_empty() {
            return Ok(None);
        }
        Ok(Some(results))
    }

    #[cfg(feature = "blocking")]
    pub fn search(&self, query: &str) -> Result<Option<SearchResults>, Error> {
        let mut results: SearchResults = vec![];
        let mut meta = SearchPageMeta::default();

        loop {
            let builder = self.get_search_builder(
                query,
                &meta,
            )?;
            let resp = builder.send().map_err(Error::ClientError)?;

            resp.error_for_status_ref()?;
            let html = resp.text().map_err(Error::ClientError)?;
            let res_page = parse_search_results(&html
            ).map_err(|e| Error::SearchError(
                format!("Failed to parse search results for {}: {:?}", query, e)
            ))?;

            match res_page {
                Some(p) => {
                    meta = p.0;
                    results.extend(p.1);
                    if !meta.has_next_page {
                        break;
                    }
                }
                None => {
                    break;
                }
            }
        }

        if results.is_empty() {
            return Ok(None);
        }
        Ok(Some(results))
    }

    /// `get_update_details` retrieves the update details for the given update id.
    /// The update id can be found in the `id` field of the `Update` struct.
    ///
    /// # Parameters
    ///
    /// * `update_id` - The update id to retrieve details for.
    ///
    /// # Example
    ///
    /// ```
    /// use msuc::Client as MsucClient;
    /// use tokio_test;
    ///
    /// tokio_test::block_on(async {
    ///     let msuc_client = MsucClient::new().expect("Failed to create MSUC client");
    ///    // MS08-067
    ///     msuc_client.get_update_details("9397a21f-246c-453b-ac05-65bf4fc6b68b").await.expect("Failed to get update details");
    /// });
    /// ```
    #[cfg(not(feature = "blocking"))]
    pub async fn get_update_details(&self, update_id: &str) -> Result<Update, Error> {
        let url = format!("{}{}", self.update_url, update_id);
        let resp = self.client
            .get(url.as_str())
            .send()
            .await
            .map_err(Error::ClientError)?;
        resp.error_for_status_ref()?;
        let html = resp.text().await.map_err(Error::ClientError)?;
        parse_update_details(&html
        ).map_err(|e| Error::SearchError(
            format!("Failed to parse update details for {}: {:?}", update_id, e)
        ))
    }

    #[cfg(feature = "blocking")]
    pub fn get_update_details(&self, update_id: &str) -> Result<Update, Error> {
        let url = format!("{}{}", self.update_url, update_id);
        let resp = self.client
            .get(url.as_str())
            .send()
            .map_err(Error::ClientError)?;
        resp.error_for_status_ref()?;
        let html = resp.text().map_err(Error::ClientError)?;
        parse_update_details(&html
        ).map_err(|e| Error::SearchError(
            format!("Failed to parse update details for {}: {}", update_id, e)
        ))
    }
}

// parse_hidden_error_page handles the case where the Microsoft Update Catalog returns a 200
// but the page contains an error message. This is a 500 from what I've seen so far.
fn parse_hidden_error_page(html: &str) -> Result<(), Error> {
    let document = Html::parse_document(html);
    let selector = Selector::parse("div#errorPageDisplayedError")
        .map_err(|e| Error::ParseError(e.to_string()))?;
    match document.select(&selector).next() {
        Some(e) => {
            // the error is format is: "[Error number: 8DDD0010]"
            let error_code = e
                .text()
                .collect::<String>()
                .trim()
                .trim_start_matches("[Error number: ")
                .trim_end_matches(']')
                .to_string();
            Err(Error::MsucError("received 500 error from Microsoft Update Catalog".to_string(), error_code))
        },
        None => Ok(()),
    }
}

fn parse_search_results(html: &str) -> Result<Option<SearchPage>, Error> {
    let document = Html::parse_document(html);
    parse_hidden_error_page(html)?;

    // The current page places the results in a table within a div container in
    let selector = Selector::parse(r#"div#tableContainer tr"#)
        .map_err(|e| Error::ParseError(e.to_string()))?;
    let mut results = SearchResults::new();
    for row in document.select(&selector) {
        let id = row
            .value()
            .attr("id")
            .ok_or(Error::ParseError("Failed to find id attribute for search result element".to_string()))?;
        if id.eq("headerRow") {
            continue;
        }

        let (update_id, row_id) = parse_search_row_id(id)?;
        let title = get_search_row_text(
            &row,
            SearchResColumn::Title,
            update_id,
            row_id,
        )?;
        results.push(SearchResult {
            title: title.to_string(),
            id: update_id.to_string(),
            kb: parse_kb_from_string(title)?,
            product: get_search_row_text(
                &row,
                SearchResColumn::Product,
                update_id,
                row_id,
            )?,
            classification: get_search_row_text(
                &row,
                SearchResColumn::Classification,
                update_id,
                row_id,
            )?,
            last_modified: parse_update_date(
                get_search_row_text(
                    &row,
                    SearchResColumn::LastUpdated,
                    update_id,
                    row_id,
                )?
            )?,
            version: parse_optional_string(
                get_search_row_text(
                    &row,
                    SearchResColumn::Version,
                    update_id,
                    row_id,
                )?
            ),
            size: parse_size_from_mb_string(
                get_search_row_text(
                    &row,
                    SearchResColumn::Size,
                    update_id,
                    row_id,
                )?
                    // There is an original size in the response, but for consistency
                    // we'll use the string representation of the size that's also
                    // on the update details page
                    .split('\n')
                    .next()
                    .ok_or(Error::ParseError("Failed to parse size".to_string()))?
                    .trim()
                    .to_string()
            )?,
        });
    }

    if results.is_empty() {
        return Ok(None);
    }
    let has_next_page = select_with_path(&document, "#ctl00_catalogBody_nextPageLinkText").is_ok();

    Ok(Some((
        SearchPageMeta {
            event_target: if has_next_page {
                // The static next page value
                "ctl00$catalogBody$nextPageLinkText".to_string()
            } else {
                "".to_string()
            },
            event_argument: get_element_attr(&document, "#__EVENTARGUMENT", "value").unwrap_or_else(|_| "".to_string()),
            event_validation: get_element_attr(&document, "#__EVENTVALIDATION", "value").unwrap_or_else(|_| "".to_string()),
            view_state: get_element_attr(&document, "#__VIEWSTATE", "value")?,
            view_state_generator: get_element_attr(&document, "#__VIEWSTATEGENERATOR", "value").unwrap_or_else(|_| "".to_string()),
            // If this element exists, there is a next page
            has_next_page,
            too_many_results: select_with_path(&document, "#ctl00_catalogBody_moreResults").is_ok(),
        },
        results,
    ))
    )
}

fn parse_update_details(html: &str) -> Result<Update, Error> {
    let document = Html::parse_document(html);
    // The current page places the results in a table within a div container in
    let u = Update {
        title: select_with_path(&document, "#ScopedViewHandler_titleText")?,
        id: select_with_path(&document, "#ScopedViewHandler_UpdateID")?,
        kb: format!(
            "KB{}",
            clean_nested_div_text(
                select_with_path(&document, "div#kbDiv")?
            )?
        ),
        classification: clean_nested_div_text(
            select_with_path(&document, "#classificationDiv")?
        )?,
        last_modified: parse_update_date(
            select_with_path(&document, "#ScopedViewHandler_date")?
        )?,
        size: parse_size_from_mb_string(
            select_with_path(&document, "#ScopedViewHandler_size")?
        )?,
        description: select_with_path(&document, "#ScopedViewHandler_desc")?,
        architecture: parse_optional_string(
            clean_nested_div_text(
                select_with_path(&document, "#archDiv")?
            )?
        ),
        supported_products: parse_nested_div_list(&document, "#productsDiv")?,
        supported_languages: parse_nested_div_list(&document, "#languagesDiv")?,
        msrc_number: parse_optional_string(
            clean_nested_div_text(
                select_with_path(&document, "#securityBullitenDiv")?
            )?
        ),
        msrc_severity: parse_optional_string(
            select_with_path(&document, "#ScopedViewHandler_msrcSeverity")?
        ),
        info_url: Url::parse(
            &select_with_path(&document, "#moreInfoDiv a")?
        )
            .map_err(|e| Error::ParseError(e.to_string()))?,
        support_url: Url::parse(
            // There is a typo in the ID of this element 'suportUrlDiv'
            &select_with_path(&document, "#suportUrlDiv a")?
        )
            .map_err(|e| Error::ParseError(e.to_string()))?,
        reboot_behavior: parse_reboot_behavior(
            select_with_path(&document, "#ScopedViewHandler_rebootBehavior")?
        )?,
        requires_user_input: parse_yes_no_bool(
            select_with_path(&document, "#ScopedViewHandler_userInput")?
        )?,
        is_exclusive_install: parse_yes_no_bool(
            select_with_path(&document, "#ScopedViewHandler_installationImpact")?
        )?,
        requires_network_connectivity: parse_yes_no_bool(
            select_with_path(&document, "#ScopedViewHandler_connectivity")?
        )?,
        uninstall_notes: parse_optional_string(clean_string_with_newlines(
            select_with_path(&document, "#uninstallNotesDiv div")?
        )),
        uninstall_steps: parse_optional_string(
            select_with_path(&document, "#uninstallStepsDiv div")?
        ),
        supersedes: get_update_supercedes_updates(&document)?,
        superseded_by: get_update_superseded_by_updates(&document)?,
    };

    Ok(u)
}

fn get_element_text(element: &scraper::ElementRef) -> Result<String, Error> {
    let t: String = element
        .text()
        .collect();
    Ok(t.trim().to_string())
}

fn get_element_attr(document: &Html, path: &str, attr: &str) -> Result<String, Error> {
    let selector = Selector::parse(path)
        .map_err(|e| Error::ParseError(e.to_string()))?;
    document
        .select(&selector)
        .next()
        .ok_or(Error::ParseError(format!("Failed to find element with selector '{}'", path)))?
        .value()
        .attr(attr)
        .ok_or(Error::ParseError(format!("Failed to find attribute '{}' for element", attr)))
        .map(|s| s.to_string())
}

fn select_with_path(document: &Html, path: &str) -> Result<String, Error> {
    let selector = Selector::parse(path)
        .map_err(|e| Error::ParseError(e.to_string()))?;
    document
        .select(&selector)
        .next()
        .ok_or(Error::ParseError(format!("Failed to find element with selector '{}'", path)))
        .and_then(|e| get_element_text(&e))
}

#[derive(Eq, PartialEq, Debug)]
enum SearchResColumn {
    Title,
    Product,
    Classification,
    LastUpdated,
    Version,
    Size,
}

fn clean_nested_div_text(text: String) -> Result<String, Error> {
    Ok(text
        .split('\n')
        .last()
        .ok_or(Error::ParseError("Failed to clean div text".to_string()))?
        .trim()
        .to_string())
}

fn parse_nested_div_list(document: &Html, path: &str) -> Result<Vec<String>, Error> {
    Ok(
        select_with_path(document, path)?
            .split('\n')
            .filter_map(|s| {
                let s = s.trim();
                // filter the first label element and empty string/rows
                if s.is_empty() || s.ends_with(':') || s == "," {
                    None
                } else {
                    Some(s.to_string())
                }
            })
            .collect())
}

fn parse_optional_string(s: String) -> Option<String> {
    match s.as_str() {
        "n/a" => None,
        _ => Some(s.to_string()),
    }
}

fn parse_reboot_behavior(s: String) -> Result<RebootBehavior, Error> {
    match s.as_str() {
        "Required" => Ok(RebootBehavior::Required),
        "Can request restart" => Ok(RebootBehavior::CanRequest),
        "Recommended" => Ok(RebootBehavior::Recommended),
        "Not required" => Ok(RebootBehavior::NotRequired),
        "Never restarts" => Ok(RebootBehavior::NeverRestarts),
        _ => Err(Error::ParseError(format!("Failed to parse reboot behavior from '{}'", s))),
    }
}

fn parse_yes_no_bool(s: String) -> Result<bool, Error> {
    match s.as_str() {
        "Yes" => Ok(true),
        "No" => Ok(false),
        "" => Ok(false),
        _ => Err(Error::ParseError(format!("Failed to parse requires user input from '{}'", s))),
    }
}

fn parse_update_date(date: String) -> Result<chrono::NaiveDate, Error> {
    chrono::NaiveDate::parse_from_str(date.as_str(), "%m/%d/%Y")
        .map_err(|e| Error::ParseError(e.to_string()))
}

fn parse_kb_from_string(s: String) -> Result<String, Error> {
    Ok(format!(
        "KB{}",
        s.split("(KB").last()
            .ok_or(Error::ParseError("Failed to find KB number in title".to_string()))?
            .split(')').next()
            .ok_or(Error::ParseError("Failed to parse KB number from title".to_string()))?)
    )
}

fn parse_size_from_mb_string(s: String) -> Result<u64, Error> {
    Ok(s.split(' ').next()
        .ok_or(Error::ParseError("Failed to parse size from MB string".to_string()))?
        // There's a decimal point in the size, cheap way to remove it
        .replace('.', "")
        .parse::<u64>()
        .map_err(|e: ParseIntError| Error::ParseError(e.to_string()))?
        // divide by ten to account for the decimal point
        * 1024 * 1024 / 10)
}

fn parse_search_row_id(id: &str) -> Result<(&str, &str), Error> {
    let mut parts: Vec<&str> = id.split("_R").take(2).collect();

    match parts.len() {
        2 => Ok((parts.remove(0), parts.remove(0))),
        _ => Err(Error::ParseError(format!("Failed to parse row id from '{}'", id))),
    }
}

/// `clean_string_with_newlines` removes newlines and extra whitespace from a string
/// while preserving the original whitespace.
fn clean_string_with_newlines(s: String) -> String {
    s.split('\n')
        .map(|s| s.trim().to_string())
        .collect::<Vec<String>>()
        .join(" ")
}

fn get_update_superseded_by_updates(document: &Html) -> Result<Vec<SupersededByUpdate>, Error> {
    let selector = Selector::parse(r#"div#supersededbyInfo div a"#)
        .map_err(|e| Error::ParseError(e.to_string()))?;
    let mut superseded_by = vec![];
    for row in document.select(&selector) {
        let title = clean_string_with_newlines(
            get_element_text(&row)?
        );
        let id = row
            .value()
            .attr("href")
            .ok_or(Error::ParseError("Failed to find id attribute for superseded by update element".to_string()))?
            .trim_start_matches("ScopedViewInline.aspx?updateid=");
        superseded_by.push(SupersededByUpdate {
            title: title.to_string(),
            kb: parse_kb_from_string(title)?,
            id: id.to_string(),
        });
    }
    Ok(superseded_by)
}

fn get_update_supercedes_updates(document: &Html) -> Result<Vec<SupersedesUpdate>, Error> {
    let selector = Selector::parse(r#"div#supersedesInfo div"#)
        .map_err(|e| Error::ParseError(e.to_string()))?;
    let mut supersedes = vec![];
    for row in document.select(&selector) {
        let title = clean_string_with_newlines(
            get_element_text(&row)?
        );
        supersedes.push(SupersedesUpdate {
            title: title.to_string(),
            kb: parse_kb_from_string(title)?,
        });
    }
    Ok(supersedes)
}

fn get_search_row_selector(column: &SearchResColumn, update_id: &str, row_id: &str) -> Result<Selector, Error> {
    let column_id = match column {
        SearchResColumn::Title => 1,
        SearchResColumn::Product => 2,
        SearchResColumn::Classification => 3,
        SearchResColumn::LastUpdated => 4,
        SearchResColumn::Version => 5,
        SearchResColumn::Size => 6,
    };
    // Need to split the first two characters of the update_id to get the valid selector
    let update_id_split = update_id.split_at(1);
    // If the first character is a number, we need to escape it based on its unicode value
    if update_id_split.0
        .chars()
        .next()
        .ok_or(Error::ParseError("the update_id is empty".to_string()))?
        .is_numeric() {
        return Selector::parse(
            &format!(r#"td#\3{} {}_C{}_R{}"#, update_id_split.0, update_id_split.1, column_id, row_id)
        ).map_err(|e| Error::ParseError(e.to_string()));
    }
    Selector::parse(
        &format!(r#"td#{}_C{}_R{}"#, update_id, column_id, row_id)
    ).map_err(|e| Error::ParseError(e.to_string()))
}

fn get_search_row_text(element: &scraper::ElementRef, column: SearchResColumn, update_id: &str, row_id: &str) -> Result<String, Error> {
    let selector = get_search_row_selector(&column, update_id, row_id)?;
    let t: String = element
        .select(&selector)
        .next()
        .ok_or(
            Error::ParseError(
                format!("no result for id '{}', column '{:?}', row '{}' with given selector '{:?}'", update_id, &column, row_id, selector)
            )
        )?
        .text()
        .collect();
    Ok(t.trim().to_string())
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("reqwest error: {0}")]
    ClientError(#[from] reqwest::Error),
    #[error("parse error: {0}")]
    ParseError(String),
    #[error("search error: {0}")]
    SearchError(String),
    #[error("internal error: {0}")]
    InternalError(String),
    #[error("msuc error: {0}, code: {1}")]
    MsucError(String, String),
}

#[cfg(test)]
mod test {
    use chrono::NaiveDate;
    use url::Url;
    use super::*;
    macro_rules! load_test_data {
        ($fname:expr) => (
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/resources/test/", $fname))
            .expect(format!("Failed to load test data from {}", $fname).as_str())
        )
    }

    #[test]
    fn test_parse_valid_search_results() {
        let test_cases = [
            (
                load_test_data!("msuc_small_result.html"),
                (SearchPageMeta {
                    event_target: "".to_string(),
                    event_argument: "".to_string(),
                    event_validation: "".to_string(),
                    view_state: "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".to_string(),
                    view_state_generator: "".to_string(),
                    has_next_page: false,
                    too_many_results: false,
                },
                 vec![
                     SearchResult {
                         title: "Security Update For Exchange Server 2019 CU12 (KB5030524)".to_string(),
                         id: "56a97db8-1478-4860-a935-7996c78d10be".to_string(),
                         kb: "KB5030524".to_string(),
                         product: "Exchange Server 2019".to_string(),
                         classification: "Security Updates".to_string(),
                         last_modified: NaiveDate::from_ymd_opt(2023, 8, 15).expect("Failed to parse date for test data"),
                         version: None,
                         size: 168715878,
                     },
                     SearchResult {
                         title: "Security Update For Exchange Server 2019 CU13 (KB5030524)".to_string(),
                         id: "70c08420-a012-4f5b-9b48-95a6b177d34a".to_string(),
                         kb: "KB5030524".to_string(),
                         product: "Exchange Server 2019".to_string(),
                         classification: "Security Updates".to_string(),
                         last_modified: NaiveDate::from_ymd_opt(2023, 8, 15).expect("Failed to parse date for test data"),
                         version: None,
                         size: 168715878,
                     },
                     SearchResult {
                         title: "Security Update For Exchange Server 2016 CU23 (KB5030524)".to_string(),
                         id: "a08b526d-3947-4ddd-ba72-a8244b39c611".to_string(),
                         kb: "KB5030524".to_string(),
                         product: "Exchange Server 2016".to_string(),
                         classification: "Security Updates".to_string(),
                         last_modified: NaiveDate::from_ymd_opt(2023, 8, 15).expect("Failed to parse date for test data"),
                         version: None,
                         size: 165045862,
                     },
                 ],
                )
            ),
            (
                load_test_data!("msuc_double_digit_rows.html"),
                (SearchPageMeta {
                    event_target: "".to_string(),
                    event_argument: "".to_string(),
                    event_validation: "".to_string(),
                    view_state: "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".to_string(),
                    view_state_generator: "".to_string(),
                    has_next_page: false,
                    too_many_results: false,
                },
                 vec![
                     SearchResult {
                         title: "2023-09 Cumulative Update for Windows 10 Version 21H2 for x64-based Systems (KB5030211)".to_string(),
                         id: "453112b9-83bb-403c-9263-018ffe515016".to_string(),
                         kb: "KB5030211".to_string(),
                         product: "Windows 10 LTSB, Windows 10,  version 1903 and later".to_string(),
                         classification: "Security Updates".to_string(),
                         last_modified: NaiveDate::from_ymd_opt(2023, 9, 12).expect("Failed to parse date for test data"),
                         version: None,
                         size: 802160640,
                     },
                     SearchResult {
                         title: "2023-09 Dynamic Cumulative Update for Windows 10 Version 21H2 for ARM64-based Systems (KB5030211)".to_string(),
                         id: "97fcb38d-dcb2-41e7-b75b-96327b676926".to_string(),
                         kb: "KB5030211".to_string(),
                         product: "Windows 10 and later GDR-DU".to_string(),
                         classification: "Security Updates".to_string(),
                         last_modified: NaiveDate::from_ymd_opt(2023, 9, 12).expect("Failed to parse date for test data"),
                         version: None,
                         size: 811912396,
                     },
                     SearchResult {
                         title: "2023-09 Dynamic Cumulative Update for Windows 10 Version 21H2 for x64-based Systems (KB5030211)".to_string(),
                         id: "0aec0f4e-5228-4f59-bfc4-08e3c3cd32bb".to_string(),
                         kb: "KB5030211".to_string(),
                         product: "Windows 10 and later GDR-DU".to_string(),
                         classification: "Security Updates".to_string(),
                         last_modified: NaiveDate::from_ymd_opt(2023, 9, 12).expect("Failed to parse date for test data"),
                         version: None,
                         size: 785697996,
                     },
                     SearchResult {
                         title: "2023-09 Cumulative Update for Windows 10 Version 21H2 for ARM64-based Systems (KB5030211)".to_string(),
                         id: "c0e5f33a-0509-4891-9935-438d061b806e".to_string(),
                         kb: "KB5030211".to_string(),
                         product: "Windows 10 LTSB, Windows 10,  version 1903 and later".to_string(),
                         classification: "Security Updates".to_string(),
                         last_modified: NaiveDate::from_ymd_opt(2023, 9, 12).expect("Failed to parse date for test data"),
                         version: None,
                         size: 827221606,
                     },
                     SearchResult {
                         title: "2023-09 Dynamic Cumulative Update for Windows 10 Version 22H2 for ARM64-based Systems (KB5030211)".to_string(),
                         id: "cdf18eed-1b04-4211-87a0-d0e865ea16ba".to_string(),
                         kb: "KB5030211".to_string(),
                         product: "Windows 10 and later GDR-DU".to_string(),
                         classification: "Security Updates".to_string(),
                         last_modified: NaiveDate::from_ymd_opt(2023, 9, 12).expect("Failed to parse date for test data"),
                         version: None,
                         size: 811912396,
                     },
                     SearchResult {
                         title: "2023-09 Cumulative Update for Windows 10 Version 22H2 for ARM64-based Systems (KB5030211)".to_string(),
                         id: "7ef071f6-f25c-457a-bd10-d0dcfb149cd0".to_string(),
                         kb: "KB5030211".to_string(),
                         product: "Windows 10,  version 1903 and later".to_string(),
                         classification: "Security Updates".to_string(),
                         last_modified: NaiveDate::from_ymd_opt(2023, 9, 12).expect("Failed to parse date for test data"),
                         version: None,
                         size: 827221606,
                     },
                     SearchResult {
                         title: "2023-09 Cumulative Update for Windows 10 Version 22H2 for x86-based Systems (KB5030211)".to_string(),
                         id: "7969059c-6aad-4562-a40f-8c764af68e86".to_string(),
                         kb: "KB5030211".to_string(),
                         product: "Windows 10,  version 1903 and later".to_string(),
                         classification: "Security Updates".to_string(),
                         last_modified: NaiveDate::from_ymd_opt(2023, 9, 12).expect("Failed to parse date for test data"),
                         version: None,
                         size: 439772774,
                     },
                     SearchResult {
                         title: "2023-09 Cumulative Update for Windows 10 Version 21H2 for x86-based Systems (KB5030211)".to_string(),
                         id: "1e3b4e94-a544-4137-8fba-8ae1a2853a95".to_string(),
                         kb: "KB5030211".to_string(),
                         product: "Windows 10 LTSB, Windows 10,  version 1903 and later".to_string(),
                         classification: "Security Updates".to_string(),
                         last_modified: NaiveDate::from_ymd_opt(2023, 9, 12).expect("Failed to parse date for test data"),
                         version: None,
                         size: 439772774,
                     },
                     SearchResult {
                         title: "2023-09 Cumulative Update for Windows 10 Version 22H2 for x64-based Systems (KB5030211)".to_string(),
                         id: "4aec4d66-a06c-4544-9f79-55ace822e015".to_string(),
                         kb: "KB5030211".to_string(),
                         product: "Windows 10,  version 1903 and later".to_string(),
                         classification: "Security Updates".to_string(),
                         last_modified: NaiveDate::from_ymd_opt(2023, 9, 12).expect("Failed to parse date for test data"),
                         version: None,
                         size: 802160640,
                     },
                     SearchResult {
                         title: "2023-09 Dynamic Cumulative Update for Windows 10 Version 22H2 for x86-based Systems (KB5030211)".to_string(),
                         id: "403e7eb7-6022-4197-bf50-65aeca4ff368".to_string(),
                         kb: "KB5030211".to_string(),
                         product: "Windows 10 and later GDR-DU".to_string(),
                         classification: "Security Updates".to_string(),
                         last_modified: NaiveDate::from_ymd_opt(2023, 9, 12).expect("Failed to parse date for test data"),
                         version: None,
                         size: 432118169,
                     },
                     SearchResult {
                         title: "2023-09 Dynamic Cumulative Update for Windows 10 Version 21H2 for x86-based Systems (KB5030211)".to_string(),
                         id: "590018dd-2c62-42b7-bd0b-e065f9283f36".to_string(),
                         kb: "KB5030211".to_string(),
                         product: "Windows 10 and later GDR-DU".to_string(),
                         classification: "Security Updates".to_string(),
                         last_modified: NaiveDate::from_ymd_opt(2023, 9, 12).expect("Failed to parse date for test data"),
                         version: None,
                         size: 432118169,
                     },
                     SearchResult {
                         title: "2023-09 Dynamic Cumulative Update for Windows 10 Version 22H2 for x64-based Systems (KB5030211)".to_string(),
                         id: "aaba42ce-ba39-4d0a-94af-0f51e68d5bfb".to_string(),
                         kb: "KB5030211".to_string(),
                         product: "Windows 10 and later GDR-DU".to_string(),
                         classification: "Security Updates".to_string(),
                         last_modified: NaiveDate::from_ymd_opt(2023, 9, 12).expect("Failed to parse date for test data"),
                         version: None,
                         size: 785697996,
                     },
                 ],
                )
            ),
        ];

        for tc in test_cases.iter() {
            let results = parse_search_results(tc.0.as_str());
            assert!(results.is_ok());
            let page = results.unwrap();
            assert!(page.is_some());
            let page = page.unwrap();
            assert_eq!(tc.1.1.len(), page.1.len());
            assert_eq!(tc.1.0.event_argument.to_string(), page.0.event_argument);
            assert_eq!(tc.1.0.event_target.to_string(), page.0.event_target);
            assert_eq!(tc.1.0.view_state.to_string(), page.0.view_state);
            assert_eq!(tc.1.0.has_next_page, page.0.has_next_page);
            assert_eq!(tc.1.0.too_many_results, page.0.too_many_results);
            for (i, u) in tc.1.1.iter().enumerate() {
                assert_eq!(u, &page.1[i]);
            }
        }
    }

    #[test]
    fn test_parse_hidden_error_search_results() {
        let test_cases = [
            (
                load_test_data!("msuc_search_error_500.html"),
                "msuc error: received 500 error from Microsoft Update Catalog, code: 8DDD0010"
            )

        ];

        for tc in test_cases.iter() {
            let results = parse_search_results(tc.0.as_str());
            assert!(results.is_err());
            match results {
                Err(e) => {
                    assert_eq!(tc.1, e.to_string());
                },
                _ => {
                    panic!("Expected error to be returned");
                }
            }
        }
    }

    #[test]
    fn test_parse_search_with_next_page() {
        let data = load_test_data!("msuc_search_with_next_page.html");
        let meta = SearchPageMeta {
            event_target: "ctl00$catalogBody$nextPageLinkText".to_string(),
            event_argument: "".to_string(),
            event_validation: "Q57xOoxbkNk6CIlyl8ZPyh/5fNLK3G9FQMGREkshZmt4tE889kCEDA0+ABemdMt1ZInO4weW8vRSBHbKYriDFif1NNzliGmGxOXhznNvDU0iW0VgaS1Zwia6t+Z65VEH/qYCkLNEWQ7dNcpJX+fwilMUNGTzyBNovNMj/wPuS77z/Arlz++phVH4J4UAJ8Bx+tdBl/M2hXMA8Ied1UA7xtOqAHWSORsYKuS29TjeFQBel45kPwngTHdvDtAJTSYfspkmiGu04rV3iWqwGEzGW/i0UM+DBLJv3XORC9yDg4g=".to_string(),
            view_state: "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".to_string(),
            view_state_generator: "BBBC20B8".to_string(),
            has_next_page: true,
            too_many_results: false,
        };

        let results = parse_search_results(data.as_str());
        assert!(results.is_ok());
        let page = results.unwrap();
        assert!(page.is_some());
        let page = page.unwrap();
        // just check the length, other tests will check the contents
        assert_eq!(25, page.1.len());
        assert_eq!(meta, page.0);
    }

    #[test]
    fn test_parse_search_too_many_results() {
        let data = load_test_data!("msuc_search_too_many_results.html");
        let meta = SearchPageMeta{
            event_target: "ctl00$catalogBody$nextPageLinkText".to_string(),
            event_argument: "".to_string(),
            event_validation: "975cSztH5m9SBl9/WBBMMiO0RbMmhIXHQSoNRcGwVvcn4nM18ly2Aj9Hkl+LNozX5x2ieNvue1S/AgWpztavOAqVQFRxu3G3WQcqpn18G1ABYA5lP9CSdZ+UYWDaPytlvqESvfGX2IjSIASH38B5bR369G/T/ltjOgSl43f1RjgblNyxpwGGofmD/3kP0W7qW0djGX+F81+dNDuJqlmaA6Tp/nWgxNXQ3duYjFUWGZu08SHR0ojoIVEYZM/PZtFv5/INp2FVHvD6B3UQ/yacHL0jcfa7n1/1NSeALa8y9GA=".to_string(),
            view_state: "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".to_string(),
            view_state_generator:  "BBBC20B8".to_string(),
            has_next_page: true,
            too_many_results: true,
        };

        let results = parse_search_results(data.as_str());
        assert!(results.is_ok());
        let page = results.unwrap();
        assert!(page.is_some());
        let page = page.unwrap();
        // just check the length, other tests will check the contents
        assert_eq!(25, page.1.len());
        assert_eq!(meta, page.0);
    }

    #[test]
    fn test_parse_update_details() {
        let test_cases = [
            (
                load_test_data!("msuc_update_details.html"),
                Update {
                    title: "2023-04 Cumulative Update Preview for Windows 11 Version 22H2 for x64-based Systems (KB5025305)".to_string(),
                    id: "1b0b70c0-191e-42f6-8808-c1b50deacb3b".to_string(),
                    kb: "KB5025305".to_string(),
                    classification: "Updates".to_string(),
                    last_modified: NaiveDate::from_ymd_opt(2023, 4, 25).expect("Failed to parse date for test data"),
                    size: 331559731,
                    description: "Install this update to resolve issues in Windows. For a complete listing of the issues that are included in this update, see the associated Microsoft Knowledge Base article for more information. After you install this item, you may have to restart your computer.".to_string(),
                    architecture: None,
                    supported_products: vec!["Windows 11".to_string()],
                    supported_languages: vec!["Arabic".to_string(), "Bulgarian".to_string(), "Czech".to_string(), "Danish".to_string(), "German".to_string(), "Greek".to_string(), "English".to_string(), "Spanish".to_string(), "Estonian".to_string(), "Finnish".to_string(), "French".to_string(), "Hebrew".to_string(), "Croatian".to_string(), "Hungarian".to_string(), "Italian".to_string(), "Japanese".to_string(), "Korean".to_string(), "Lithuanian".to_string(), "Latvian".to_string(), "Norwegian".to_string(), "Dutch".to_string(), "Polish".to_string(), "Portuguese (Brazil)".to_string(), "Portuguese (Portugal)".to_string(), "Romanian".to_string(), "Russian".to_string(), "Slovak".to_string(), "Slovenian".to_string(), "Serbian (Latin)".to_string(), "Swedish".to_string(), "Thai".to_string(), "Turkish".to_string(), "Ukrainian".to_string(), "Chinese (Simplified)".to_string(), "Chinese (Traditional)".to_string(), "all".to_string()],
                    msrc_number: None,
                    msrc_severity: None,
                    info_url: Url::parse("https://support.microsoft.com/help/5025305").expect("Failed to parse URL for test data"),
                    support_url: Url::parse("https://support.microsoft.com/help/5025305").expect("Failed to parse URL for test data"),
                    reboot_behavior: RebootBehavior::CanRequest,
                    requires_user_input: false,
                    is_exclusive_install: false,
                    requires_network_connectivity: false,
                    uninstall_notes: None,
                    uninstall_steps: None,
                    supersedes: vec![
                        SupersedesUpdate {
                            title: "2023-04 Cumulative Update for Windows 11 Version 22H2 for x64-based Systems (KB5025239)".to_string(),
                            kb: "KB5025239".to_string(),
                        },
                        SupersedesUpdate {
                            title: "2023-02 Cumulative Update Preview for Windows 11 Version 22H2 for x64-based Systems (KB5022913) UUP".to_string(),
                            kb: "KB5022913".to_string(),
                        },
                        SupersedesUpdate {
                            title: "2023-03 Cumulative Update Preview for Windows 11 Version 22H2 for x64-based Systems (KB5023778)".to_string(),
                            kb: "KB5023778".to_string(),
                        },
                        SupersedesUpdate {
                            title: "2022-09 Cumulative Update Preview for Windows 11 Version 22H2 for x64-based Systems (KB5017389)".to_string(),
                            kb: "KB5017389".to_string(),
                        },
                        SupersedesUpdate {
                            title: "2022-10 Cumulative Update for Windows 11 Version 22H2 for x64-based Systems (KB5018427)".to_string(),
                            kb: "KB5018427".to_string(),
                        },
                        SupersedesUpdate {
                            title: "2022-10 Cumulative Update for Windows 11 Version 22H2 for x64-based Systems (KB5019509)".to_string(),
                            kb: "KB5019509".to_string(),
                        },
                        SupersedesUpdate {
                            title: "2022-09 Cumulative Update for Windows 11 Version 22H2 for x64-based Systems (KB5017321)".to_string(),
                            kb: "KB5017321".to_string(),
                        },
                        SupersedesUpdate {
                            title: "2022-09 Cumulative Update for Windows 11 Version 22H2 for x64-based Systems (KB5019311)".to_string(),
                            kb: "KB5019311".to_string(),
                        },
                        SupersedesUpdate {
                            title: "2022-11 Cumulative Update for Windows 11 Version 22H2 for x64-based Systems (KB5019980)".to_string(),
                            kb: "KB5019980".to_string(),
                        },
                        SupersedesUpdate {
                            title: "2023-01 Cumulative Update for Windows 11 Version 22H2 for x64-based Systems (KB5022303)".to_string(),
                            kb: "KB5022303".to_string(),
                        },
                        SupersedesUpdate {
                            title: "2023-01 Cumulative Update Preview for Windows 11 Version 22H2 for x64-based Systems (KB5022360)".to_string(),
                            kb: "KB5022360".to_string(),
                        },
                        SupersedesUpdate {
                            title: "2022-11 Cumulative Update Preview for Windows 11 Version 22H2 for x64-based Systems (KB5020044)".to_string(),
                            kb: "KB5020044".to_string(),
                        },
                        SupersedesUpdate {
                            title: "2023-02 Cumulative Update Preview for Windows 11 Version 22H2 for x64-based Systems (KB5022913)".to_string(),
                            kb: "KB5022913".to_string(),
                        },
                        SupersedesUpdate {
                            title: "2022-10 Cumulative Update Preview for Windows 11 Version 22H2 for x64-based Systems (KB5018496)".to_string(),
                            kb: "KB5018496".to_string(),
                        },
                        SupersedesUpdate {
                            title: "2022-12 Cumulative Update for Windows 11 Version 22H2 for x64-based Systems (KB5021255)".to_string(),
                            kb: "KB5021255".to_string(),
                        },
                        SupersedesUpdate {
                            title: "2023-02 Cumulative Update for Windows 11 Version 22H2 for x64-based Systems (KB5022845)".to_string(),
                            kb: "KB5022845".to_string(),
                        },
                        SupersedesUpdate {
                            title: "2023-03 Cumulative Update for Windows 11 Version 22H2 for x64-based Systems (KB5023706)".to_string(),
                            kb: "KB5023706".to_string(),
                        },
                    ],
                    superseded_by: vec![
                        SupersededByUpdate {
                            title: "2023-09 Cumulative Update for Windows 11 Version 22H2 for x64-based Systems (KB5030219)".to_string(),
                            kb: "KB5030219".to_string(),
                            id: "03423c5a-458d-4cbe-b67e-d47bec7f3fb6".to_string(),
                        },
                        SupersededByUpdate {
                            title: "2023-08 Cumulative Update for Windows 11 Version 22H2 for x64-based Systems (KB5029263)".to_string(),
                            kb: "KB5029263".to_string(),
                            id: "10b0cdce-d084-452d-b6a3-318a3ade0a6e".to_string(),
                        },
                        SupersededByUpdate {
                            title: "2023-08 Cumulative Update Preview for Windows 11 Version 22H2 for x64-based Systems (KB5029351)".to_string(),
                            kb: "KB5029351".to_string(),
                            id: "1a1ab822-a9e3-4a00-abd5-a4fafbf02982".to_string(),
                        },
                        SupersededByUpdate {
                            title: "2023-07 Cumulative Update for Windows 11 Version 22H2 for x64-based Systems (KB5028185)".to_string(),
                            kb: "KB5028185".to_string(),
                            id: "1f6417e4-a329-42c4-95e0-fa7d09bb6f90".to_string(),
                        },
                        SupersededByUpdate {
                            title: "2023-05 Cumulative Update for Windows 11 Version 22H2 for x64-based Systems (KB5026372)".to_string(),
                            kb: "KB5026372".to_string(),
                            id: "3cf3be77-f086-449f-8ba5-033f605c688a".to_string(),
                        },
                        SupersededByUpdate {
                            title: "2023-07 Cumulative Update Preview for Windows 11 Version 22H2 for x64-based Systems (KB5028254)".to_string(),
                            kb: "KB5028254".to_string(),
                            id: "dbf7dc02-70ef-4476-b228-00a130a39ccd".to_string(),
                        },
                        SupersededByUpdate {
                            title: "2023-06 Cumulative Update Preview for Windows 11 Version 22H2 for x64-based Systems (KB5027303)".to_string(),
                            kb: "KB5027303".to_string(),
                            id: "e0c1bca2-82c9-4eca-b0b2-5c5a507a683a".to_string(),
                        },
                        SupersededByUpdate {
                            title: "2023-06 Cumulative Update for Windows 11 Version 22H2 for x64-based Systems (KB5027231)".to_string(),
                            kb: "KB5027231".to_string(),
                            id: "eac58b58-fb7d-4cd4-a78a-a39f87e0f232".to_string(),
                        },
                        SupersededByUpdate {
                            title: "2023-05 Cumulative Update Preview for Windows 11 Version 22H2 for x64-based Systems (KB5026446)".to_string(),
                            kb: "KB5026446".to_string(),
                            id: "ec3769c8-2cd5-4e89-a0a3-6e7830c38f6f".to_string(),
                        },
                    ],
                }
            ),
            (
                load_test_data!("msuc_update_details_never_restarts.html"),
                Update {
                    title: "Security Update For Exchange Server 2019 CU12 (KB5030524)".to_string(),
                    id: "56a97db8-1478-4860-a935-7996c78d10be".to_string(),
                    kb: "KB5030524".to_string(),
                    classification: "Security Updates".to_string(),
                    last_modified: NaiveDate::from_ymd_opt(2023, 8, 15).expect("Failed to parse date for test data"),
                    size: 168715878,
                    description: "The security update addresses the vulnerabilities descripted in the CVEs".to_string(),
                    architecture: None,
                    supported_products: vec!["Exchange Server 2019".to_string()],
                    supported_languages: vec!["Arabic".to_string(), "Bulgarian".to_string(), "Chinese (Traditional)".to_string(), "Czech".to_string(), "Danish".to_string(), "German".to_string(), "Greek".to_string(), "English".to_string(), "Spanish".to_string(), "Finnish".to_string(), "French".to_string(), "Hebrew".to_string(), "Hungarian".to_string(), "Italian".to_string(), "Japanese".to_string(), "Korean".to_string(), "Dutch".to_string(), "Norwegian".to_string(), "Polish".to_string(), "Portuguese (Brazil)".to_string(), "Romanian".to_string(), "Russian".to_string(), "Croatian".to_string(), "Slovak".to_string(), "Swedish".to_string(), "Thai".to_string(), "Turkish".to_string(), "Ukrainian".to_string(), "Slovenian".to_string(), "Estonian".to_string(), "Latvian".to_string(), "Lithuanian".to_string(), "Hindi".to_string(), "Chinese (Simplified)".to_string(), "Portuguese (Portugal)".to_string(), "Serbian (Latin)".to_string(), "Chinese - Hong Kong SAR".to_string(), "Japanese NEC".to_string()],
                    msrc_number: None,
                    msrc_severity: None,
                    info_url: Url::parse("https://techcommunity.microsoft.com/t5/exchange-team-blog/bg-p/Exchange").expect("Failed to parse URL for test data"),
                    support_url: Url::parse("https://technet.microsoft.com/en-us/exchange/fp179701").expect("Failed to parse URL for test data"),
                    reboot_behavior: RebootBehavior::NeverRestarts,
                    requires_user_input: false,
                    is_exclusive_install: false,
                    requires_network_connectivity: false,
                    uninstall_notes: Some("This software update can be removed via Add or Remove Programs in Control Panel.".to_string()),
                    uninstall_steps: None,
                    supersedes: vec![
                        SupersedesUpdate {
                            title: "Security Update For Exchange Server 2019 CU12 (KB5026261)".to_string(),
                            kb: "KB5026261".to_string(),
                        },
                        SupersedesUpdate {
                            title: "Security Update For Exchange Server 2019 CU12 (KB5024296)".to_string(),
                            kb: "KB5024296".to_string(),
                        }],
                    superseded_by: vec![],
                }
            )
        ];
        for tc in test_cases.iter() {
            let res = parse_update_details(&tc.0);
            assert!(res.is_ok());
            let res = res.unwrap();
            assert_eq!(tc.1, res);
        }
    }
}