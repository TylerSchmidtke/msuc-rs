use std::num::ParseIntError;
use scraper::{Html, Selector};
use thiserror::Error;

const LIB_VERSION: &str = env!("CARGO_PKG_VERSION");

/// `Update` represents a single update from the Microsoft Update Catalog.
#[derive(Eq, PartialEq, Debug)]
pub struct Update {
    pub title: String,
    pub id: String,
    pub kb: String,
    pub product: String,
    pub classification: String,
    pub last_updated: chrono::NaiveDate,
    pub version: Option<String>,
    pub size: u64,
}

/// SearchResults represents a collection of updates returned from a search.
pub type SearchResults = Vec<Update>;

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

    // TODO: Implement parsing of the update details page
    /*
    pub async fn get_update(&self, update_id: &str) -> Result<String, Error> {
        let url = format!("{}{}", self.update_url, update_id);
        let resp = self.client
            .get(url.as_str())
            .send()
            .await
            .map_err(Error::ClientError)?;
        resp.text().await.map_err(Error::ClientError)
    }
    */
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

        let (update_id, row_id) = parse_row_id(id)?;
        let title = get_update_row_text(
            &row,
            UpdateColumn::Title,
            update_id,
            row_id,
        )?;
        let last_updated = get_update_row_text(
            &row,
            UpdateColumn::LastUpdated,
            update_id,
            row_id,
        )?;
        let size_str = get_update_row_text(
            &row,
            UpdateColumn::Size,
            update_id,
            row_id,
        )?
            // The original_size is hidden in the html, but available within the _size td element.
            // When parsing, this text gets extracted but on separate lines. We only want the last
            .split('\n')
            .last()
            .ok_or(Error::ParseError("Failed to parse size".to_string()))?
            .trim()
            .to_string();
        results.push(Update {
            title: title.to_string(),
            id: update_id.to_string(),
            kb: format!("KB{}", title
                .split("(KB").last()
                .ok_or(Error::ParseError("Failed to find KB number in title".to_string()))?
                .split(')').next()
                .ok_or(Error::ParseError("Failed to parse KB number from title".to_string()))?
            ),
            product: get_update_row_text(
                &row,
                UpdateColumn::Product,
                update_id,
                row_id,
            )?,
            classification: get_update_row_text(
                &row,
                UpdateColumn::Classification,
                update_id,
                row_id,
            )?,
            last_updated: chrono::NaiveDate::parse_from_str(last_updated.as_str(), "%m/%d/%Y")
                .expect("Failed to parse date"),
            version: match get_update_row_text(
                &row,
                UpdateColumn::Version,
                update_id,
                row_id,
            )?.as_str() {
                "n/a" => None,
                v => Some(v.to_string()),
            },
            size: size_str
                .parse()
                .map_err(|e: ParseIntError| Error::ParseError(e.to_string()))?,
        });
    }

    if results.is_empty() {
        return Ok(None);
    }

    Ok(Some(results))
}

#[derive(Eq, PartialEq, Debug)]
enum UpdateColumn {
    Title,
    Product,
    Classification,
    LastUpdated,
    Version,
    Size,
}

fn parse_row_id(id: &str) -> Result<(&str, &str), Error> {
    let mut parts: Vec<&str> = id.split("_R").take(2).collect();

    match parts.len() {
        2 => Ok((parts.remove(0), parts.remove(0))),
        _ => Err(Error::ParseError(format!("Failed to parse row id from '{}'", id))),
    }
}

fn get_update_row_selector(column: &UpdateColumn, update_id: &str, row_id: &str) -> Result<Selector, Error> {
    let column_id = match column {
        UpdateColumn::Title => 1,
        UpdateColumn::Product => 2,
        UpdateColumn::Classification => 3,
        UpdateColumn::LastUpdated => 4,
        UpdateColumn::Version => 5,
        UpdateColumn::Size => 6,
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

fn get_update_row_text(element: &scraper::ElementRef, column: UpdateColumn, update_id: &str, row_id: &str) -> Result<String, Error> {
    let selector = get_update_row_selector(&column, update_id, row_id)?;
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
                    Update {
                        title: "Security Update For Exchange Server 2019 CU12 (KB5030524)".to_string(),
                        id: "56a97db8-1478-4860-a935-7996c78d10be".to_string(),
                        kb: "KB5030524".to_string(),
                        product: "Exchange Server 2019".to_string(),
                        classification: "Security Updates".to_string(),
                        last_updated: NaiveDate::from_ymd_opt(2023, 8, 15).expect("Failed to parse date for test data"),
                        version: None,
                        size: 168724351,
                    },
                    Update {
                        title: "Security Update For Exchange Server 2019 CU13 (KB5030524)".to_string(),
                        id: "70c08420-a012-4f5b-9b48-95a6b177d34a".to_string(),
                        kb: "KB5030524".to_string(),
                        product: "Exchange Server 2019".to_string(),
                        classification: "Security Updates".to_string(),
                        last_updated: NaiveDate::from_ymd_opt(2023, 8, 15).expect("Failed to parse date for test data"),
                        version: None,
                        size: 168755833,
                    },
                    Update {
                        title: "Security Update For Exchange Server 2016 CU23 (KB5030524)".to_string(),
                        id: "a08b526d-3947-4ddd-ba72-a8244b39c611".to_string(),
                        kb: "KB5030524".to_string(),
                        product: "Exchange Server 2016".to_string(),
                        classification: "Security Updates".to_string(),
                        last_updated: NaiveDate::from_ymd_opt(2023, 8, 15).expect("Failed to parse date for test data"),
                        version: None,
                        size: 165033099,
                    },
                ],
            ),
            (
                load_test_data!("msuc_double_digit_rows.html"),
                vec![
                    Update {
                        title: "2023-09 Cumulative Update for Windows 10 Version 21H2 for x64-based Systems (KB5030211)".to_string(),
                        id: "453112b9-83bb-403c-9263-018ffe515016".to_string(),
                        kb: "KB5030211".to_string(),
                        product: "Windows 10 LTSB, Windows 10,  version 1903 and later".to_string(),
                        classification: "Security Updates".to_string(),
                        last_updated: NaiveDate::from_ymd_opt(2023, 9, 12).expect("Failed to parse date for test data"),
                        version: None,
                        size: 802153202,
                    },
                    Update {
                        title: "2023-09 Dynamic Cumulative Update for Windows 10 Version 21H2 for ARM64-based Systems (KB5030211)".to_string(),
                        id: "97fcb38d-dcb2-41e7-b75b-96327b676926".to_string(),
                        kb: "KB5030211".to_string(),
                        product: "Windows 10 and later GDR-DU".to_string(),
                        classification: "Security Updates".to_string(),
                        last_updated: NaiveDate::from_ymd_opt(2023, 9, 12).expect("Failed to parse date for test data"),
                        version: None,
                        size: 811959866,
                    },
                    Update {
                        title: "2023-09 Dynamic Cumulative Update for Windows 10 Version 21H2 for x64-based Systems (KB5030211)".to_string(),
                        id: "0aec0f4e-5228-4f59-bfc4-08e3c3cd32bb".to_string(),
                        kb: "KB5030211".to_string(),
                        product: "Windows 10 and later GDR-DU".to_string(),
                        classification: "Security Updates".to_string(),
                        last_updated: NaiveDate::from_ymd_opt(2023, 9, 12).expect("Failed to parse date for test data"),
                        version: None,
                        size: 785680490,
                    },
                    Update {
                        title: "2023-09 Cumulative Update for Windows 10 Version 21H2 for ARM64-based Systems (KB5030211)".to_string(),
                        id: "c0e5f33a-0509-4891-9935-438d061b806e".to_string(),
                        kb: "KB5030211".to_string(),
                        product: "Windows 10 LTSB, Windows 10,  version 1903 and later".to_string(),
                        classification: "Security Updates".to_string(),
                        last_updated: NaiveDate::from_ymd_opt(2023, 9, 12).expect("Failed to parse date for test data"),
                        version: None,
                        size: 827189794,
                    },
                    Update {
                        title: "2023-09 Dynamic Cumulative Update for Windows 10 Version 22H2 for ARM64-based Systems (KB5030211)".to_string(),
                        id: "cdf18eed-1b04-4211-87a0-d0e865ea16ba".to_string(),
                        kb: "KB5030211".to_string(),
                        product: "Windows 10 and later GDR-DU".to_string(),
                        classification: "Security Updates".to_string(),
                        last_updated: NaiveDate::from_ymd_opt(2023, 9, 12).expect("Failed to parse date for test data"),
                        version: None,
                        size: 811959866,
                    },
                    Update {
                        title: "2023-09 Cumulative Update for Windows 10 Version 22H2 for ARM64-based Systems (KB5030211)".to_string(),
                        id: "7ef071f6-f25c-457a-bd10-d0dcfb149cd0".to_string(),
                        kb: "KB5030211".to_string(),
                        product: "Windows 10,  version 1903 and later".to_string(),
                        classification: "Security Updates".to_string(),
                        last_updated: NaiveDate::from_ymd_opt(2023, 9, 12).expect("Failed to parse date for test data"),
                        version: None,
                        size: 827189794,
                    },
                    Update {
                        title: "2023-09 Cumulative Update for Windows 10 Version 22H2 for x86-based Systems (KB5030211)".to_string(),
                        id: "7969059c-6aad-4562-a40f-8c764af68e86".to_string(),
                        kb: "KB5030211".to_string(),
                        product: "Windows 10,  version 1903 and later".to_string(),
                        classification: "Security Updates".to_string(),
                        last_updated: NaiveDate::from_ymd_opt(2023, 9, 12).expect("Failed to parse date for test data"),
                        version: None,
                        size: 439726719,
                    },
                    Update {
                        title: "2023-09 Cumulative Update for Windows 10 Version 21H2 for x86-based Systems (KB5030211)".to_string(),
                        id: "1e3b4e94-a544-4137-8fba-8ae1a2853a95".to_string(),
                        kb: "KB5030211".to_string(),
                        product: "Windows 10 LTSB, Windows 10,  version 1903 and later".to_string(),
                        classification: "Security Updates".to_string(),
                        last_updated: NaiveDate::from_ymd_opt(2023, 9, 12).expect("Failed to parse date for test data"),
                        version: None,
                        size: 439726719,
                    },
                    Update {
                        title: "2023-09 Cumulative Update for Windows 10 Version 22H2 for x64-based Systems (KB5030211)".to_string(),
                        id: "4aec4d66-a06c-4544-9f79-55ace822e015".to_string(),
                        kb: "KB5030211".to_string(),
                        product: "Windows 10,  version 1903 and later".to_string(),
                        classification: "Security Updates".to_string(),
                        last_updated: NaiveDate::from_ymd_opt(2023, 9, 12).expect("Failed to parse date for test data"),
                        version: None,
                        size: 802153202,
                    },
                    Update {
                        title: "2023-09 Dynamic Cumulative Update for Windows 10 Version 22H2 for x86-based Systems (KB5030211)".to_string(),
                        id: "403e7eb7-6022-4197-bf50-65aeca4ff368".to_string(),
                        kb: "KB5030211".to_string(),
                        product: "Windows 10 and later GDR-DU".to_string(),
                        classification: "Security Updates".to_string(),
                        last_updated: NaiveDate::from_ymd_opt(2023, 9, 12).expect("Failed to parse date for test data"),
                        version: None,
                        size: 432155005,
                    },
                    Update {
                        title: "2023-09 Dynamic Cumulative Update for Windows 10 Version 21H2 for x86-based Systems (KB5030211)".to_string(),
                        id: "590018dd-2c62-42b7-bd0b-e065f9283f36".to_string(),
                        kb: "KB5030211".to_string(),
                        product: "Windows 10 and later GDR-DU".to_string(),
                        classification: "Security Updates".to_string(),
                        last_updated: NaiveDate::from_ymd_opt(2023, 9, 12).expect("Failed to parse date for test data"),
                        version: None,
                        size: 432155005,
                    },
                    Update {
                        title: "2023-09 Dynamic Cumulative Update for Windows 10 Version 22H2 for x64-based Systems (KB5030211)".to_string(),
                        id: "aaba42ce-ba39-4d0a-94af-0f51e68d5bfb".to_string(),
                        kb: "KB5030211".to_string(),
                        product: "Windows 10 and later GDR-DU".to_string(),
                        classification: "Security Updates".to_string(),
                        last_updated: NaiveDate::from_ymd_opt(2023, 9, 12).expect("Failed to parse date for test data"),
                        version: None,
                        size: 785680490,
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
}