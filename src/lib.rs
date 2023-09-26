use std::num::ParseIntError;
use scraper::{Html, Selector};
use thiserror::Error;


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
            search_url: String::from("https://www.catalog.update.microsoft.com/Search.aspx?q="),
            update_url: String::from("https://www.catalog.update.microsoft.com/ScopedViewInline.aspx?updateid="),
        })
    }

    /// `search` performs a search against the Microsoft Update Catalog.
    ///
    /// # Parameters
    ///
    /// * `kb` - The KB number to search for, including the 'KB' prefix.
    ///
    /// # Example
    ///
    /// ```
    /// use msuc::Client as MsucClient;
    /// use tokio_test;
    ///
    /// tokio_test::block_on(async {
    ///     let msuc_client = MsucClient::new().expect("Failed to create MSUC client");
    ///     let resp = msuc_client.search("KB5030524").await.expect("Failed to search");
    ///     for update in resp.unwrap().iter() {
    ///       println!("Found update: {}", update.title);
    ///     }
    /// });
    /// ```
    #[cfg(not(feature = "blocking"))]
    pub async fn search(&self, kb: &str) -> Result<Option<SearchResults>, Error> {
        let url = format!("{}{}", self.search_url, kb);
        let resp = self.client
            .get(url.as_str())
            .send()
            .await
            .map_err(Error::ClientError)?;
        let html = resp.text().await.map_err(Error::ClientError)?;
        parse_search_results(&html
        ).map_err(|e| Error::SearchError(
            format!("Failed to parse search results for {}: {}", kb, e)
        ))
    }

    #[cfg(feature = "blocking")]
    pub fn search(&self, kb: &str) -> Result<Option<SearchResults>, Error> {
        let url = format!("{}{}", self.search_url, kb);
        let resp = self.client
            .get(url.as_str())
            .send()
            .map_err(Error::ClientError)?;
        let html = resp.text().map_err(Error::ClientError)?;
        parse_search_results(&html
        ).map_err(|e| Error::SearchError(
            format!("Failed to parse search results for {}: {}", kb, e)
        ))
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
            format!("Failed to parse update details for {}: {}", update_id, e)
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

fn parse_search_results(html: &str) -> Result<Option<SearchResults>, Error> {
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

    Ok(Some(results))
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
                }  else {
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
            ),
            (
                load_test_data!("msuc_double_digit_rows.html"),
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
                ]
            ),
        ];

        for tc in test_cases.iter() {
            let results = parse_search_results(tc.0.as_str());
            assert!(results.is_ok());
            let results = results.unwrap();
            assert!(results.is_some());
            let results = results.unwrap();
            assert_eq!(tc.1.len(), results.len());
            for (i, u) in tc.1.iter().enumerate() {
                assert_eq!(u, &results[i]);
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