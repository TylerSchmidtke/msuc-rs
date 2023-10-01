use std::collections::HashMap;
use std::num::ParseIntError;
use reqwest::{ClientBuilder, RequestBuilder};
use scraper::{Html, Selector};
use thiserror::Error;
use url::Url;
use crate::Error::{InternalError, ParseError};


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

struct SearchPageMeta {
    event_target: String,
    event_argument: String,
    event_validation: String,
    view_state: String,
    view_state_generator: String,
    has_next_page: bool,
}

impl SearchPageMeta {
    fn new(
        event_target: String,
        event_argument: String,
        event_validation: String,
        view_state: String,
        view_state_generator: String,
        has_next_page: bool,
    ) -> Self {
        SearchPageMeta {
            event_target,
            event_argument,
            event_validation,
            view_state,
            view_state_generator,
            has_next_page,
        }
    }

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
    pub info_url: url::Url,
    pub support_url: url::Url,
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
    // TODO: Implement parsing of the update details page
    #[allow(dead_code)]
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
            InternalError(format!("Failed to parse search url '{}': {}", self.search_url, e.to_string()))
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

    /// `search` performs a search against the Microsoft Update Catalog.
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
    ///
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

            // TODO: Handle hidden errors in the page, e.g 500s
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
    pub fn search(&self, kb: &str) -> Result<Option<SearchResults>, Error> {
        let mut results: SearchResults = vec![];
        let mut meta = SearchPageMeta::default();

        loop {
            let builder = self.get_search_builder(
                query,
                &meta,
            )?;
            let resp = builder.send().map_err(Error::ClientError)?;

            // TODO: Handle hidden errors in the page, e.g 500s
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
        let html = resp.text().map_err(Error::ClientError)?;
        parse_update_details(&html
        ).map_err(|e| Error::SearchError(
            format!("Failed to parse update details for {}: {}", update_id, e)
        ))
    }
}

fn parse_search_results(html: &str) -> Result<Option<SearchPage>, Error> {
    let document = Html::parse_document(html);
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
        info_url: url::Url::parse(
            &select_with_path(&document, "#moreInfoDiv a")?
        )
            .map_err(|e| Error::ParseError(e.to_string()))?,
        support_url: url::Url::parse(
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
    fn test_parse_search_results() {
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
                    view_state: "KdVyiUfV4zF++F1Kse091GOhtd4FF3eGL/K5TFrMm7H2dF/DH0ydr7eEbSW10DCfueAGDjBs18TbdJHYbWBYyEFwm2mIf8tB1yANyKddUSs/EuemmeqCg7PTMdogejVVE5nzOcaTzPiXAi2pSOhG8ue1kyZd673TL2PLnIZddVl1H0ZG/LrGtQm6kLssODwkKG5bQCFFsW8sGgLhWyuAk0J7yibwopu/ZU7lnjePU37uexB2KFf7DDc/m8LGmOC/vkNSiWJxcKw0c8k4u/mt/IUvbwc6Lw0y3ApDdC1ZYse9mIru+15THJs69VNHEXSg2jOYeCpTBwQa6KKFT2X9Z0kv3ol/HpHmpPsi2INNZJrfo1urh0Bs5xKHXWiV6Ua/LTOhmrXndWboN1nlKz2qLZGZIkEPMFBF9i3qi+cB8O1eeE4OSPEuqVt11p4l0926MB8LojlSu0Te6Ekc+U0vopOYNQE6m2U1HkXIWu4vCFDKmR/H2a95+evhrWFQW6UFdTi66WVpg1azjB4RQFzr9kG9SJcjJUIr7QMBr5a6X7Aqsui/o/QcLmtcQu2UnTUSNplYiE+keMHw/I4k0uuyyH5gnRb5BguKIE4imYVNYxy+jp+6+zLqXiD1YearDsuZo2sgq8pMQ9D2OlEL/czxYBMtauuOAtGERI/RxM3Wbj3KsiOiYmwC6+bDI+wtTgFM0NLp1Isdspz9N6GFQWV5edguIbKCTK5O2brnkKpu0WWzVjvm4o9D6m3qDgAOTMujyFavXePXSbx/5eij4sLDC6wMBVcGi9T65XRKLEvR9j96Y4vQJ8mCzaAnDIdIKwdJDL/kmWx82edDBdc3wvCkZd13k/hqRUqdr1HTrQ9zpwWtY0cJ+ww1IRHykqjmfEXVC5H8hYHTIwVGOkN40mhLCilD7joImOgyqQBSPlJRQ79mX6Lwguvl1eIrsHUrOV+CR691bONRf8qOp7jI+eUd1mub7KyoRfLjCdchMNo9HjruJOtnO5urVsnePE9a8kDmc460NKh64YQA7RmTZGqDwSfwzIRZQkKPUNtnXtZ82rlRg3Yfs8Z8B+03JPdB3AdhirkYLdEtQinM5tRS7sz4/xPX2Qn2jQJX3iR9evQpPLzweIRlv1+tXjGELJclxquCRxu+j+tSVUoeU2QazeI6fSMsCu9gx8xdszaPNfICQXIqLh8jG9J46ZRszwk5jMUEfe+edhzKkSXW57fCcnOmhUar/t21btNLVIqyCWfywc4Z0YVPT/g2IdvjaEw10ZBdYDeT9RPiZlI5X1FSaBKPIaXlciWqFMcxZEEPoze7e5TUhnLx2baGl4AOFDY+JRGRIRLENZdbB/KvP9NUPoRED0FLNfVQPoLGWzI3bQ6Tm/lu0XdKkqPkkz6FbQ6I326eLHnVuIgeUjuNBfCW4XapGPXL0PZVh+V50XWHsvdk0UmThnvN266IcqmKhOZJnztYVdjY3sFMnTz3rgPw2X6dGNhkkSL31jSzmJ1ynlUOMCdh6szYmFE2jrpxpHHr2Vr6LLc6OYisIcLcBI3z8zGV7x9M89DYUyGEoHTuEflDctmGrwNb7JujBz7Q/FENyd6wIFw5r1AlUPKtKYpyC/rVCS92LBmw5XDyqz1Sq08E/YTUEi4z467QENfZUnfXMF6d2zPd+gemn3UvRmf1godisImALRgumfiXwDDtd0yn6onzgyawDSJJDcjpU1a0tkiOjHgV3TgPG3RxeN7O6SqzeY1hgEq08T7hNn9eRXitro54u7yomr3Zzr/sTF/Qctvbb5p8ZvWCUy1GXlZ3uG9khAjt608WTNCZ+83OBKCyDr2nS0c9ONR6Oa8328CiOQyZ9pkS1li07b65RUdJ+X5eVoA1kYCWkyI4TZeukWHLKl8/ANqnvR4v77xj0/BIOoKxZxsEt8NGAeEyJaocowdEfeb1ZMDQkwvqyfaBnvZCg/wrw0vEHhYGOp+nVVUCi2xdXpi7oI8LXOXa/YWCwlqwseNkpfxia3DOBKM2nJRw68QEiRY9bgeeSqz+CcKeiCeKLMCQfi/n2m6hWphXrBPia6abS3SAsAmDDcIushh2mFyS0ukHFPDpXhLitKxLkQe1kTyxKHwcmDQubm2iLs65t7RsI8wT6hPxAVd9nSAjRWEzv35C7vYpius6C72/xbAqrornkJ3rkm2AEKdor7RtAyFsklgP5CnUUpTo1JefYvray44eRHfmIqa70EIOeNn/m9+Amq574IZC6+uiIFvkcVQFvvl03nYzVNxYFehTyWyZA31SZzY8XFWwuq3laWyLsGvweIxpoBTEcISFIfmVv1eLYgxqnvoUNpsL5iqo+Hmqm72YqlX6ExCaRMQB1wBG4gkU/EDsI0d3RlWlHrBMu/rj3mDvu4DhPHc5sZ9USQjQDttjOeZwEyltW9j9l6zdZlrkHFvS6DM4EsfOdBDMsPGiyF2zVAEz2g5KNY9IfKGUjU7oYunad+zhvqB0Mg+FizCfMsG1HV3KmeoP30trUnIBNFaMtHC4uQOSq7cuvwEy8M3A6bZYLGT0UkDvhAB+VfEz6hzT/Z6TmXV1D2HX+FB0EPSTjgDgu15k1y8s0ZVRtWi1VMffZsJ7yDhs/cHOO/1Xh8GmESTMMVqM+MF2vx2W/yT5Wo95IeHMA4Vy4bMGpfrKUXZaXwA8Dpq6Qlhh4IA+46MmjK/SeWCXHQTaxz8SVeLD87trXAF4sCGgSDzbByGas8z+s+iwFJyKYaFkK/gSAtxX0RlhBFuh8NXDo2fyR8kgrBnAlxku/C67Ner/VlPyLVNXv6/6cxWLn7pyc0xC0EsahONJewQ7WMAKtUcW+7sT5JXEZk4qJexSbftKaC1852dwjdLyExyhZ4WCobJsP5jU72WvUu/OtpC2M380r1M1I7N7gbKz/i6xNzOY4S6s7ZHnlFu0F9/0o93juCFBjHLw3JLvSUVR4JDNyM3uwML1zdXFFQLbCLZoUVwIwa0m0ykwVkaFXUrldQIASMMPbXW9240fO9MqmIDvlsdKxAqMOS6nYJhykcdeGohTv8wylBMzLI0Ywl4scl82uiYAuCW8vzLye4XbIbpbtHD1U0QP27p5L4y5Hg5Yln0MsNjw2UrSut5W8n135ZJRj3zkp8wfSAHSjfREWZqgWMOnEv3gbJurJIS5ZYfj7uH95hPeEJuOpTFGCl9R4xLErvNp3mdOeBqBHTWhXqOQ909SYQwNgI/qxlQlzZOPKJSUMILvi5bwusLEf1JvWWEOrq+DtxB5mGsZQRiRnRTAZl/spvj/r+FjerxdkUSBwjeCySIzzo3Bitq4J5FxeJ/1VPpKErjhVckcdm4pldLF8QbNiPhjAnFhyEzJtVFkE5KoFSK5pOTXXmndWZ35E9TGB/NEExnRDUNGEZYvBiaTpN+pYYg1Dgc3bOt/Qu9HHMwn2gf9SuUzqm7Ea1FrmThxvuT/LtLm5m8juh+cvCq9pxZ46dV/oLDsTO5u98JesvKhWe3+HBJlpbYNLUo7TkbDe/sTCwQr620P7VUVYfMUUoz+eLoj09ql6PURrhsh2bRCtOH5XdZlhEV33t/ZdxMPEmXgyh4PVAaBdR9kk8jI1CbH6RJMiCaxt8QPLoE2JQT71bkUwKQLMAjshu/78jN8cILVJP7I/n1+MsazximZ6cDkoXF9T/92rEW7Ncqs1anE46j3V/3R0OlZtt7D8X72k6xBB++4TX6HxdcmjI1BVRNoh3uTVzh00pOpBqMQn7AdE7bO9olyYKqkiWtwNfEF8GnWYMltBgYbPAINyNrnL/nO8VRnBCFY+Iy9aFua6zs80W4lj/mDAa4VtVIHb6EQAC39d2YwWo1Dnw9tVGwAM87DfqcUuHn1DbtuS5tnQe0MvOoVY95P4ojuD2jRq07SedH1ZgL/8Am12lOTP53Q798kpMq884rA2ugfJpGOV4W0/yD9/oJwe71QtY7t9i/Eyoz4uu+NkoNJFZ8kAQa3rH/uWeKF4f+ZNLmH6OIABeY8lmzf9tPcKLvNNuGcDFeGEc4AObB+Z4YDQTHg5diwGZb/rj/SFeLq42XzmAd97OYVrBdarKyL1K8eCae2Hv+pC64PLlh+0StnvXcQdu9jGgS57NNd/5zeeoORS4kWyU5TV7unIqSoL8Hx+R5OnG4bKzTspVvEynyKKqDpD8uWQlVAGuJu4Me5GRXGzKFp0GPWj+jJPpbz7r+gJrhA/ssMX1CjFnw5mbfMQJIHGetk4vqTob09aPuOFzVoHieplFI6T6EJiwwQruNcvRMN+k+VhNtSwXqIxoDseSFW66En4nXFVtp3Aw8/e1adCsrDNiRTaH/0ixcqE3Y3+iroNps7sLk2O6zxDk0W8bKvJsrJ+m+wsZ3hyXBky2cJ9gQ8vTBgonjLEamK3rSpig4LN4Im6VYkElfJvW60NXOuXroJooY3UDeiAss2WvkRfPnIQ4iry5AwLz8uXJO/hqZW8uPAMqPcePZ96Cyr10Q7eq6jdjr/VhkHSAM/N4X1/LKphIon3Ugl9cgmr3AtfEwOj286mqWUhl5qYYIHl3xj9yjYo6ezc85wDHgrqTBAZRPth1pReXuVfjaPT9quKk3BbHFoelXULtIT1MLGzM1kEs+yamXr/gRGkPIqzFK5lRoIZg+Tr55sGEmiUOfj7U+FB8tNtTXZ9H6srs5qQVu6EnFMr4TKoDTmwohpZhPAIsW0QWgd+GXwI2T9b7OhCPw1grjh67vDqNKS2N0ln1+D2aXzDJCUTQJDsfAtAtmsEemFiwXdlhRgQzjpQ13NEGCV5B/M6jxSe2CyLbIRPYJPk/58mzyIye5ir7GDqf8GzxIUz2V+DleKvQLL3G1cWdE3bIglonztYZZ6YUxOW6p3UdnX/oRYyAiPU0TALGvUVpQ+TKQ6ldauGX6SeXyv1VjfEzjyhnGDJRP3Yd1s6chWkABzC2F5TQyKEHWAc024YTDj3GztchZHz+gVHWNehkWkESfxH2KcoqaE9+XZHF3mpHLwU898BxsDZzKMQ5dTehFNJAsJXyBUUpf0bz6Rme1iYAtxi3v4SfA/YIjZRN2pm5cLmNwhgTohFzIz01uVj0kWgtwPh0ILk6XjqmnKc7lqHqN81slIXucPxi6YTP2xdefS55VjD8SRyZIR1lfbf8SMDrlaHFh2XWyOvx2hhXInA9ayDuG5c4MqEgoAMDRuyf1aIhUEciofi8BPjAQre7UzcbT0KpA+1cTuSNr28DQ8IyQ9RgEOtB3ZpRO78WPiEgYnqd0nLmGpo36FmwC7Alcj5au5Wki02hlEqPyjcC9caL0WvFQZyVq/i+xVfjR3z7I62oQnuefkCEEluNArow4BtwX771NNtHoyDxd9x7AbKMN7LAkapsfY+iU5CwuWYmXs77vktw+Jz6eSv+R3Fsw0fQ2woKfqXMhvFoHUqrqr4lYff0zkffLqo4iFGnUtkCK4PdoVVVVvj2t+Cx8yjs/xypbJDaKHk8MM5ygqqBUIuhkJkp1V21GbGbInrifZuMHgC+I8/M5S7qGQYtJxDCwjH7BUt5Wq1TX0MBBdqCBC82tKh4J/b+RWLtBjVw8n1zIQlj0HNbVBiEmjXjnwV4Ff6H4pSCgsTbK6ExnjZNQ4f3ILaCbvx/RH9+7QT/lRU71nvHQLl8yyED2D8tDW3KBJmAc5/eoFhiKtshMoMdHtIpTbipe5eA/LdwtpUjzBBlHS7MUWA+4LzoGKSharVevlbgI9nm2PNRWs80okrvJnH8DX6XqHNpKH0pvYyLenI9pXZbBNya0n9rUDeUnl1CVYSqV6s0tfzu/rOQBhK/UwMrzoDIEYPvroTuUSTEa32ch3n9PwlTiAIYBO1z0YcyIe3mlz0Y0L5Zm8mRNr5jRXqLSNdoEPDg76Xa5oGPuCXH38XCtRmMwjARPjyRTXasSFfOxBTH/DQJ8D2kD43ogF9M9npzfIesKbj2MD4kz3iinF5l+RMJlAJrh/SJFT9/2ne7dlIH+pqbY1zo75DjnGUITy4atccCWHhjqmrSBG0+FnbqISOd3JLdCn+WzL3XZqMIPXOSV54yAsGz8eB+QUQ7kjQM717jvDpvHE2nwVpUpccEXKtb/5m/CrkOAZFuLWpoGpzLSJ0ulnE7pi30nBVKpXwv/ZWZkFedJB3jZNzJfa54ZfNLWalplBxDhg5lE9umSnbjWazV9DU7vgCB4LTSYg77jhAm8Q9zxX7Nm6RO9T74TpZLtOKkMiJH1iMhsL6EwZkiZZszZzxMWCKb7kHUQpD8c/JzNpNkY6d6BJVbXRYSyhmcqxcFVIBa+nOnhLCiPJe6P8B5u5p4QcJVTm8Rtq7Ce+M5H+++OdPmOpNTZIhSaG4/wTHRpSpGYIGJ1EJUXGRM2kNz+Mbz1sJUPDVnw7XZ/AgXPfiecG0GJjjdj0gXgYhTetOqhfV/KKebiN7z+Rigpto7pWuFEstukkoebWvKhJ1ajMSZ49xjlnBK7qEhtXZI5ZJqGDToDQ3FOLeUrR2KVgjLEqHOWr+igSPfXD5XucmZ12PiqduH1+NfE4s2AkDqx17RbdichMX/IKbUNg/Vt+iyFVJTi7PRb3zpqsO+pTkImQXhLDJmkCMWkCl86hx+GbAyfpEAaoNG8KzFALpq2HQsJ/Q0UVVI/t3sOHexQvAUs1BZ+GYb8+bi9oHLcR8GePoLb/GZmKGDXUrq8qLCVEp0R8oW1lvivANgWISe+1LUFYWhmn5veTdtBcLdhX+qeRG6ItIHMob/U4KzFOdsEMMQrXNPF9MAz+VjuEDN4UZ7DK85va1djnEjQCtq/DCSppvgPYNpmOcYcfKAzx37IyyGr0qEr6rgG2/VEMzETVU26WmFM0Y6eHrGZ+BLW14FHp6P3nSPpLgMQpMTNa7G5uD+ypUrPQmI2tsHj6WYsMprAU0wLysmW2A5ylFNRE5jNiykGeA0CqB6HvhWBcLkucZ7rW3s4mSC5LhMC4Z727DiVmR17Q+d3EFP2Qp3ki5q9KBnQQFbio48FzAqYynTFZ5ANpiYRpyWGKX8Sb+8Memd1Ga90W2rljBKJELlqCZcOM3NCKU5izz60/DKt1+xUTxZ/o25aGlIZ22jTqIkCu0S+anMrdwXuqA0u0nP5b6Ax4YBtzJmkhLhcUJH4zmX6ZHxqBeAul/iyEc/VlQAO4cNboW4IBkKKjmQSB6q9oeMohsQ6QPqJTXIDs3ADFh5cnHsBv88N3d7du6f1fuvsd6EwlAqlXMCtQJJFf4zy0SUTfQgmSIU0NsATrWjKt+7OGFe6OdTStpHIaxoeQlQ/2zS/vknnQ+w5r2lIRFGrA1qzMEIHekeCbQdmoSzLH2bUn3qjb4odIFuBOdQjrgwrJtTF0fK2J/V0UIzdegwrdjx9dHcn2FK9DcczuJB+E7mcmuRhRSsxYIyij7QkErEKK+0eGCjx1aTv6iScpryP/QNzrD3qSmcsmRaRWvydiOV5QEnP8nt56AkDyxcvl7f0s+vKRdgGJx0ADGTWLc/Sy5qLh6//CnN/IunxatjBeZcGbIhWzuuzqYTRd2LjNfXKfnrnttxjrXndBBR1LTEoPvKd63Gexfah4FgnXefXK0iEV41Tc8LUBpaBmYYrPlqH1lLiYqsSIyzGStBQn9qET95V1KjHXqpGk4bOLx7hiiWe9gEMqJOp8UEgkvuuBF0lXfflPJzUYk5yOYaU1t5qYLP9zwCZri4UC+9fcaUaZRJhXj+PVj5AtukS8LYEkucT2yvVtr22gmbWO7Ou7APQ3rEOiTeCnxIqbjUV4PXkGa3c8+1CMDF2q/I/BVr5QRUEPdQbILXsn+wqZgiDh+pDvvP2LwzsxoC28clwW6UEcFaugEKFtSWorSjPY4UTRo0h7BDOzVT7J2cszCXbP/sebm0Oy77Jo5iA3HxQB+0XT70g0HyIkp/ZjtvH3nzEqKLFVj7zkgK404swe1/Z2uDg7aavzEiW5hxxJ/MufF3canPtuwmZ1cQFiWxI+fR3T2ePntKoF3zX/d01oEtuJ/PC4kSQxH1fbCBDitL2oSBu6+qbyNNELRLcDIS5x0GBstJ0RvrWjHi2kocLxQ2DXqQ/hgCSXX018LbFSS1IFV6F+jlwJX1KmBW354zXxtbx2sOPaQgNXcsb0uldluDOMUyXw5DxOBbCCZmyMmmq9oe8ZBDdzx5MAzvJB0/VKsYr+Z6PCGVGthpGFxE4pcsSSW1t4+ZsiOqUZkOWqJgsePkWPWLSzx50nMohT8qLvCVe4ksve9CFPb7VFD3CItmGPWbMh8w8ZAgrA7h9Ls8H3XN/ulMSYhjTfPAwswhQuc2kdojxvZvZmwxwBHLGj91+AIlSdroiX/T7tJUZTbfC6sSnBG6pASSmGMBl4EennHwIvoOON8/K+R1amRS6ImPzKf+ldmYWgmAEK0LlJtDtqNhIEj+is1WStPqmVZ/IONzHO5e+p0QVxnNC/nU2ygWfo2MgxqCkepGCMU6dhFrKKlyJ3tAeEMSLVe5cPMeOSFVAkFTXs9AO04nCQZmpaWnHznzpthikgJ19b9E+T6c1SaYoRC+0WwV4lflZJbus/uwgCMChUZXBqt1Eq/9g3PwOWfOWrcF9KYVBHukKEREAPFZlkzU9Y6pj7kRImHWTecb2/ezDtq+S2wB+c5nmoKuHS8btFPPsEO+Y89KB6/XaPeCZrkq/yx4xPwR0BqAOB5soE9MyO7M4pLHjCr6vSC00anRy25DzjPFbg3gNUpniKnDwOrj9bClqwwPqAuvc3iM16NWKL10cpsiteynSFS3QWPG1YQ0bycrDNnoCyf4nnptQeCCZdmuK5ojHDpHi0LHvE6+Fn0sXlyTj0MM8yWBRKUQJ/kePZJshK65RgrpGAnwHuzNcm/LuxpibEz1C3Huosz5Rt6sfY5pfC3k+FW0RsuvSZvdF7FaNbGZoHzk7B8vQqbWySZLtp2fh9BtDhFAY7uhGRkPvfFh0iZudp5sCWFhE7nygg8yUAL07rca2F1ShWZRhqGxDefMi1PwoN943KgRp74Iu9t+3NHrj6tytgo/hgzcFXJi+X+oACbDf9uDepSf2hVR8JoSYJJiY6vbYKxyVbQ7kEspeu5VDsr3j09htHHH8dJkSahSF02cZaf+5qCinFFQbaE8uesuvFgOFgjirZDnGXrAOsF00I4nmbZ0BE8JrWFuB2pA5MpFx9TrA5iWM8CTOQzhUaMzGGX/8EMgvHzbrfp5zwmrWszuIsAZuvZOpYd8CeNj0twr4sgjlAjX6iUkWdRRnNE4r65gmEOniaUycXW5GJ3iwHAawwplZLkqMxft9NoTi4B+VtZytNJLSKNPYruKKZZP8MXPvP3FzDstH+q2kgUCim8AqI+B7A4WyUHvlMKzYwcSrvwt7Cx7ztIVXOu6y3RKYXebO8PwN3cIPqup/4k5ItIWtPYVM13kH66hsNy8yheL5rcDgySP3jXp/oJlJwM810QBJY4PPug9fr/OBsBwxpFVE7rEbltu6Whdbr7kAm9PPbv1qCqkDsahjvRVMEHVAZb8vhQ+XRNfCZWZT5VelWidpVYbpWu1Orz3NTeNSpRY5uKoEpX7eC5DMeSjOmgczOOyAdQuaKLwfVruCcu2vuOPZK3b02vs/0TRgIEYI7rnQxdfAKblNcTzQZBPb5uGXB1qbdxtk3DVO4FYumC10YcjqGsDRvjTCUBHP4wh0M7qmX8+ia2GRSUoOScJHllUbVFSe7Zb+D8EVNogqbr93Z9SdQC/Rbjq7IEBdGN6YAhEpc2XrLbO/t48ht8xuE513V7nnmvnMAaphxwRF4cJXg5sNdN8jD51707Li7/6L/Vl54pTH+3NT2MFMlQIIaMRfvyUoGLha8a0ZhOR5N6kziYAs7ng24zJkOPSDgBAf/p7TgjqbmNCt2HXdfezZX1kD8pIcHbDx6GBj08S/lUBEFgz75sI3KBfBKTBEk83p1pxS6w6hq7ErXHs6zmR8HBSQYZ9ju3+y0R0Cb2suaddRoRVMk+8fkOIG/LlXJiUnh+uyVVxFceOP5UrIg07opqqzgqS0O4aePepM5yNORUWggi3ofrkMDWOf1zoZfMS5Mwu3JGv50Vcvau+YtkaZfCfe5rG67ZTMosk9sydVLaAqfzAwtR0IBbeD9cvUyBI0lzk7wTwut2MGuheOCvt2L7jID0Dq4qSLX2i5XChaaIjiZ06dHNjHJgc4daRi1o2aHWJnPnZ5KHUAOfMAS/6OdanPPKMJXlLC0G/8UITTMPNhYY+g5RSTyJIIAtLHAnpgfY46Jx8Z0rg+yoon3PndpkiY6tg8RzRLDpH3STtV7gO+LIyeOdhOWBKzRrBP8bvunCCu27pmfqAUpv8aUGmih8TiZBUKLU9rDGpJYTKwZcSdMDQ3Ne/DXSU6ahGjfh3QUmwNu2Y7NPwUlENpwGDEhYHAAKY4Fh6ZU7Ip6Rt4gnVmB7vVyEszgM2bXABQ+n5M/i8DDhZEgzeYwX3yqKMhP5hWbyKGS+5RYTUA6eSWxtIFwJGvLDuCQGjaol19y2bsGgZ0W/IH4CWnkDVorXN6HHKr2rvKOKsKLWstwhJqsIqFqjwofn6aa9NW5eXSzO2wGgGZH0sS2gyhXuG0t0gkFgn+ym3TyjqGbppLGTVitSDuYqTvTCQPlXVfyTZtKFuOqUWZ+QlWtbQQyJxBYB9LmkWjPp63QLc3PgI4lht86SL2chmgKPu/UTnWli6vpE5SiNFFDlJCRubXDJCfc3SD7K2jnK89ZbyXAz6vppzyjtsuzqHSaQ32OzjpBMykID+gsz4Ya/0eVp+krC6UiYcHLR7naiIc78Xt6kTaEESwJwiHKWTV0QrRI9LIvDelqd0VPun6AFVwN0Fc2HuA26VN/5T9CA3mXaH6AYsTF9RCXaqQoXzHQLDxbEhReEu5RZJGHqqmwZ+N1UG5DXuzH74KYx7o4xh2x6SBDCB7KPDEniMpbMxyNbJuH3lHz/0Ar0KBDMsS6OTfIvQd6JtEKkfUuixe92qJ6z5nvEIVjRZ2dEjUa6P4bxgD+j2iCuBWqkhFGcqZD7xFRh5aBNO3NkrCTf/IpCbmOAr0/R9OS4i+gqRgKujzmYZgxtsAfUIRQkCgREuGfpK91V+hPIEewgOSLQE/L/7xO9/gInII+MVSNm9Ij42MohtvRaNuwa7qlbH7SpnhKFtuZx9Vx3G4rc1tMfo66c3lVDYBAz6nmeqaZX86pjfrirAD6WtiwlaS9xW9t09m5t/J6KsODvUuY8Z+dRwyVFAQ1UoyYKjaIZiFymHo/edw7z6KjyWjjQu7lhVH+5xER/hBHgMQlUGpekhbpcxLnXjlFbIN+62beKvaf+J72XJEjsSlcwA3v20qzgqgLbtVcD8BCZZnCImL42ry4lGIAXSP+1kA5l4v2ZkkC7ycgc3VLPIAFabZg/B9V+Q5KtMET1CgyoTMF6rTCmcr0HEjKRY+YMCSSbReOxGW4VXARKBvaxAehN8Wvd5alkFKzcKzqjioZ/UweaevqYtnxvuWcnezAe7U0cjy73BaHhIkbuaigjuT0Z7e3F94CEa2K9wnff1sMER56faQZayRyCBrQPnJHSjdTVuXW5iPBx+rtkdlxldLbllJCE2TxnBjDn8IRAYmh9utI/jZe+YzznzrnTwBbw/FSEBqwQjLz3LsgYYY91265nZ5ajumaouZ7Dzuxhb3DorAPsyWaKJBB8uiZp3BQhdZpN7MmDEm1UWYVBDPJ8+C1phfGWvvA+DwSFb32WZaQ7+0nuXHSYRUhYeu4jyurEZPIE1rV94Wrp2gXlRkE5zqgM6hRJQdxJq0Ei8sFCPKhrvXY0f2lNQ7L8HDhkYL4a24ejgtIHqs8kiODtF99IQAH3vwAMCKK73XdCTUsZktG/5776jkUMCp/YzZbvZDv2X4vqCQXrF8KCFVaEYrNddP9RRpIz+qi32G6K2UemAJeLRi5GuojckjppHTgyhSz6YQ798vRiCKossDGzLh7iVBNcmPOVhK3jdQGoi0nHeaqVw1VV6LFC2o9z1e6lxCVNmUDKZGgVQiwf1mEre7ufWsG9mpmFF0yOfJQCRVOlZZdSaPZPuVrtd4oa4nnmym/z9fq5CImCO8WpZoVlBA5WYE5tXc2FukLZ7/rSmGm4d7Ijz0hPaj6cWILx0qQ/c5MZ4YbDI2wGapHF86V+QMiuhEwInYJon9+DL1XJE3p6JtjQQiSj8gjrWKl51gE1txRwa8RYUEdA4KoNuaHsQq9WLuHcqABlkSUhHloKOFWh0cZJ7d63oH0+k2FhkYIN3/3EdkvBlSYx/aKmK3k5Y0hg2nVwsXrDij5KFo/7phnVKdA3O886eSUkeRV0veaqmgxJWq3K+0AsQnHBhlTQpyW46mYUEpYhSR/PPDfJohp/abWkz4Yyz4cV3WfYVHmv7hJs6SE9P5hgxSYTO4HUAaLaO1n7l81o3vs/SblaZC/aVhOpH33odh87iwBBX0svEDynQ0VeSgTxwFJxIbgl6pkrZ1LqFtC6Yy9piJMjokE9ipvNYk794og4wBHvubsFgxr2GNdKsimezdLg0xk6V8/0/gkWHHG7Y1XzLefFBMSS4hdCxouDI9Otk7h/vCRF2uaBtiFB2KYrK/h92UUcu4EpbTxfdj5EJapn89co/hKmIauGVXBrNIAUUkMOsXXjzgxtvH5UwcRgqGUuMkupfmIa+pnkhsw8zwjoYHzvxQTsLpQ41IluId5v38hCeE00otoyTXx8gpd6HAbbcs3aU9IevRd7JyIBvq8GfGbGWf4dgoap++neb59HQXT8UtpccS7JzO0jGXhO/Uw2t+Cl0KYD7ZJZ4ylCge5Is3i4YDP+Z1wmC1r5OfWNhNHcgzCvGguzsJCKeBX7znKG3SPkEiLlBYc82vIclnRvPwOFQ02SxNh0SQc0lnJFB8XyslYb5x3oYfqQom9pnAW/5qFmOGQGAcJ5tjHSHnxZNOxh54a44SN/IMUzlZswf4XukYeTs1sTJaWNJG8clZzGTScHfb60peeyeiSkjutPpt+zv53FaN0vY1kcAhGqeIvb32F13osv2Ab88zkDOTa6S05rC9uaDaGaC7Kf6IU/PK06LY6cPd/2ABr1s1Upq3uqwghX3sN9UXNhkR5sMo+FINXwoW+sSXyf/u2ruBp92W9hE1a0veH/cPzPwZaoGzU8FHX8RyDtG9swpTFdxPj7nq8enufNftpPgvMuYNsgb6UfAB/LW3AmWeeV8qGpheTk/wZ0NBlkS1ieBB+V3zepKx8iYHmK5imEU/cVYX9qSk3LCDz50/jWOXQQcUDiMBG/Sa+q0oBkWwxJDxwjd8mH5OJWalQXg+kELYjH8CsLukXX4GN6jFZEAi2JIkF5Gy83UJB8hlM+RcscMTJp/oW8agn5sLtPfEs8EiwWvoIBOAze3uSmJaJEQTBhGSnv8eRmTvMoAS+fpBJr0Up2vtTUQdR1fjlZVxB2HrcxRaXUASaicmx9b62gb8fjv8Q4PZuLLRBbPfaA2EaJtuhrmkjd8rNznRC3hZo22wrRRtyMZayA0qeq6xKZHYJQz09kWM58tusWwj01nXc8hWeVwc4koptQXrgpmKXARzNbmWoerBTHcD5PiCeEgWuVNxWrzNPFK/qAjE82fknH6YLbjXZjWGOX9g0SN52XIcCXX1WWLnkUaNxqaG1VxLVundz2pOdhz31613/0/fKw8t9qvqCCQtR7TTe7XEQVTbkx2v7LkBDHoH/QDu+yt6ht/Mw94qQfzZ5aZ1VnwVqeAauc8CajK0WODRMcwkEEaTo4ADDdV09Ivyd1mTDNLFJSxLnUeWAnJHDb2MCm6vpapX2tN9yhcdNP+ZqJHdsajz0xuKyqtb72Rxc+uFD8v52VHaeamWpP6M4xoKqqa/RN/1ISbzIx5fmU5sSeEEsffvLgqSkAVYiDBzmwCtiibDDlxouvGzF0++39lPU8wL6BYoUvIf5l0GJSJUY7lCHlM0CCavhnON4AWMrT6Z8xfzTirTtjujwP5P5CFECFoiCNq4hsmsCgKTWsIrCjLrGCt8nlC3I+eWX5UeNHGI6cYujqpwKyjyJqDwOv4SG5ZLVzxWM/Zz73OxAo+v1QZVfhc/UEz29oSs/ShXaYpR4pXCXTEl/eViPXVeMnMFpZn099RApkMVdrMPwC6TpK9GZVnwxVvATPcW2ARaLtW+Wc7gg+Q6RE8tzw4a81WSHmrPX4Ync0SdbLAJGhcBVNce29nA9R9vefV6UuiMhGGkKXsednpOZpKtT/1hyd3vT7gjOWlFaCX/kzALH38AV/MQebd8zGnYw3Sjcub4EMWCD4lMebgkL13BiVasJfEiim9rpdSXMXWCjV31+HTQWwT3jzG6yU22vC9xNVdF8bgIoGlqQcgQZrgbp597jS5Uur9nOlVX6mPC69klWqYVvzhFmdOqXTNSroECU5uivuzVZaAcb6TbMEoLqLB81matlSDUEn6LjVo2/jksECPa1tjM1IRksXocNFUXDv6BYNrPvgD4+PoKEO6o0LeLzrwwfn2x0vkI3hFkvnXvjuZMM1ogVDwC4+Rm1WPvUMKAMrVva74Im2It65fJDuIifpJW8Bsex0mv7ZXGCbq1gJEqyPTiFU8qNuUYC+3HmhP2+7tUv3Ph7koR0u4qqPfzj3izeVTInFt4Xu0ozuH0nHs1u85DRH9v7HIfnNZazMBBVgOgp0Dp/GzU/SNRDDprnMqa8IDOV5gDDRJUtrDN74qewCV4jGr14a8Lg+FnwLkSgivEJG+qsr3p5fYWOn+nvbJtwECAch1/zMD0NMDX7WhwZxj7DksorB1P+Y8HChPrPX048j/liVe6xKNknbPUDxlVyjNC7Jd/PLX0HxzErYhusxyHbjUJL77A8I/m5I9nxCERn7Fn5Ydw2X1jJP8kkby9jATG3290Ns6WtTkf43NzK49ZpH5XhinezeLhALnH+z7tVipFNVdvwpdwBiCNL+iU4+5j0XpSL9lf+3eZ1ZstD1xXO4/hCHwS8pqQEkHdfJssG+dBgUPfBhOdXyT4rPRyNd1T/vxBBj5TP75iQbJ4bvOjURd71aAQQwsKhy8R62PD1pZvyNiqzyGKReMbqDSuBY6nXTRK0dNPr+bnJ0G+ENC+IgBrbrJhQGhsKJZJsOW+P7r0NztoGzzDh816+//Hs/eErYMTvPBYopwvbMJ24Xd+6ljGZA4dcPjFGryMqXLhh8nmjyrT2n+BNwNo57Iy7wENGqOQwt+vboYo1ddg8SewGXZLKmyXjr5WgIWqIhoSbQVGY7yv45y/yKR9CN7rv0UqvdSQYrOvDZpRySQsHbugBaw6A+bU6BM4GE08ZYJ3pcpDk8wBeKFDXiUv75RByhlwnq7fkdnyWF9WcRpoenF+Wo/WwErlwDGVAk7LZX3e6qsFRoH/LEqGPr4xUYFnKH0Y4sFhFjtQLWKAC2sPtoFPJ+1coalcd1o8iqrVk1gVeUfSezTky/cFA4L3Fp9ZRnjmDBJ1I6mirxIhb36cPaC2uX29ZJtOXCRaX7reEXIJhCi2c1X93Ee/4klDWjB5QSY+BWQrtC+jVf59rYz31Kx4dz01EIqLn1+sUZHkGMDiG8GszGhXJ3ELsBU5WZIRKogtHM5a8gKUTewkapYzhiQISMvrciN1AfiL3hwSOYcliBRRlZ6H8jPcfk4Z5ym4f2b6p0FfVlQZiFNgvP7qPfycP+QnS+pJ8qbQsfbUqrh1BIldHOd8EJjg+Tgl7grmlDWtBtL1pn+rpybE0pAANMZ2sY/EMqaFzjaeTFfQzUqBmru/UhP7LRTElCO5pwLb2KnCHtxBORAaYBWqkEGK+8kkYiLpt+mWfmNYguCb+pYOu9yZUw/VnAvkoe99q0O2vbKi7h2JuxujSD1Zs3ZBXZFExkX0/7MJwJPWsm7GnJVMABUWag2DKtpJAw1YafJ8zSLnj0V1bxEe57cnsP5SEE/nrim30mkCKgFTWxp60WT0qHwEh6akajoU/JzM8Jhyf7zFGp1OdstVegIq6TgVdS6gegFu8YoGg8uwurr2hqIthzD5lq2Qwab7zHhMBqaGz5WsVjZEjxdUrayCJsAvA0W8aUf8Vlz/apRM5q/I0WIzPaWvCPeSvIZWjcI/ocCpVpvT0t1uVhIU31LHWKhSELOQvtmQYeKByETE39eX/MUfHNM+bPElXSXPWW8ipPyqRwSTn/Y3kkBYJ5a2u1CDo+W9SeJmwgnczB+WxbvEPjR4tjMfx6do0G2oBtRVuWjAXPcN7OW88iFFvxmR99It/t7GkaQraxie8rtbIWKXs5cZtMjIcsAx5piv44jY12z+80H/srlCQzAdscPjrBiy8vKYNZXKavowYXJmqcvyQnzE=".to_string(),
                    view_state_generator: "".to_string(),
                    has_next_page: false,
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
            for (i, u) in tc.1.1.iter().enumerate() {
                assert_eq!(u, &page.1[i]);
            }
        }
    }

    #[test]
    fn test_parse_search_with_next_page() {
        let test_cases = [
            (
                load_test_data!("msuc_search_with_next_page.html"),
                (SearchPageMeta {
                    event_target: "".to_string(),
                    event_argument: "".to_string(),
                    event_validation: "".to_string(),
                    view_state: "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".to_string(),
                    view_state_generator: "".to_string(),
                    has_next_page: false,
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
            for (i, u) in tc.1.1.iter().enumerate() {
                assert_eq!(u, &page.1[i]);
            }
        }
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