#[cfg(not(feature = "blocking"))]
use async_trait::async_trait;
#[cfg(feature = "blocking")]
use reqwest::blocking::RequestBuilder;
#[cfg(not(feature = "blocking"))]
use reqwest::RequestBuilder;
use url::Url;
use crate::model::{Error, SearchPageMeta, SearchResult, Update};
use crate::parser::{parse_search_results, parse_update_details};

const LIB_VERSION: &str = env!("CARGO_PKG_VERSION");

/// `SearchResultsStream` represents an stream of update pages returned from a search.
pub struct SearchResultsStream {
    client: Client,
    query: String,
    meta: SearchPageMeta,
}

#[cfg(not(feature = "blocking"))]
#[async_trait]
pub trait SearchResultsStreamer {
    async fn next(&mut self) -> Result<Option<Vec<SearchResult>>, Error>;
}

#[cfg(feature = "blocking")]
pub trait SearchResultsStreamer {
    fn next(&mut self) -> Result<Option<Vec<SearchResult>>, Error>;
}

impl SearchResultsStream {
    fn new(meta: SearchPageMeta, query: &str) -> Result<Self, Error> {
        Ok(SearchResultsStream {
            client: Client::new()?,
            query: query.to_string(),
            meta,
        })
    }

    /// `result_count` returns the total number of results for the search.
    pub fn result_count(&self) -> i16 {
        self.meta.pagination.result_count
    }

    /// `page_count` returns the total number of pages for the search.
    pub fn page_count(&self) -> i16 {
        self.meta.pagination.page_count
    }

    /// `current_page` returns the current page number for the search.
    pub fn current_page(&self) -> i16 {
        self.meta.pagination.current_page
    }

    /// `too_many_results` returns true if the search contains more than 1000 results which is the
    /// maximum number of results the Microsoft Update Catalog will return for a search.
    pub fn too_many_results(&self) -> bool {
        self.meta.pagination.too_many_results
    }

    /// `has_next_page` returns true if there are more pages of results to retrieve.
    pub fn has_next_page(&self) -> bool {
        self.meta.pagination.has_next_page
    }

    fn process_search_page(&mut self, html: String) -> Result<Option<Vec<SearchResult>>, Error> {
        let page = parse_search_results(&html).map_err(|e| {
            self.meta.pagination.has_next_page = false;
            Error::Search(format!(
                "Failed to parse search results for {}: {:?}",
                self.query, e
            ))
        })?;
        match page {
            Some(p) => {
                self.meta.event_target = p.0.event_target;
                self.meta.event_argument = p.0.event_argument;
                self.meta.event_validation = p.0.event_validation;
                self.meta.view_state = p.0.view_state;
                self.meta.view_state_generator = p.0.view_state_generator;
                self.meta.pagination.has_next_page = p.0.pagination.has_next_page;
                self.meta.pagination.too_many_results = p.0.pagination.too_many_results;
                Ok(Some(p.1))
            }
            None => {
                self.meta.pagination.has_next_page = false;
                Ok(None)
            }
        }
    }
}

#[cfg(not(feature = "blocking"))]
#[async_trait]
impl SearchResultsStreamer for SearchResultsStream {
    async fn next(&mut self) -> Result<Option<Vec<SearchResult>>, Error> {
        if !self.has_next_page() {
            return Ok(None);
        }
        let builder = self.client.get_search_builder(&self.query, &self.meta)?;
        let resp = builder.send().await.map_err(Error::Client)?;
        resp.error_for_status_ref()?;
        let html = resp.text().await.map_err(Error::Client)?;
        self.process_search_page(html)
    }
}

#[cfg(feature = "blocking")]
impl SearchResultsStreamer for SearchResultsStream {
    fn next(&mut self) -> Result<Option<Vec<SearchResult>>, Error> {
        if !self.has_next_page() {
            return Ok(None);
        }
        let builder = self.client.get_search_builder(&self.query, &self.meta)?;
        let resp = builder.send().map_err(Error::Client)?;
        resp.error_for_status_ref()?;
        let html = resp.text().map_err(Error::Client)?;
        self.process_search_page(html)
    }
}

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
    /// use msuc::prelude::*;
    /// let msuc_client = MsucClient::new().expect("Failed to create MSUC client");
    /// ```
    pub fn new() -> Result<Self, Error> {
        #[cfg(not(feature = "blocking"))]
            let client = reqwest::Client::builder()
            .user_agent(format!("msuc-rs/{}", LIB_VERSION))
            .build()
            .map_err(Error::Client)?;
        #[cfg(feature = "blocking")]
            let client = reqwest::blocking::Client::builder()
            .user_agent(format!("msuc-rs/{}", LIB_VERSION))
            .build()
            .map_err(Error::Client)?;

        Ok(Client {
            client,
            search_url: String::from("https://www.catalog.update.microsoft.com/Search.aspx"),
            update_url: String::from(
                "https://www.catalog.update.microsoft.com/ScopedViewInline.aspx?updateid=",
            ),
        })
    }

    fn get_search_builder(
        &self,
        query: &str,
        meta: &SearchPageMeta,
    ) -> Result<RequestBuilder, Error> {
        let mut u = Url::parse(&self.search_url).map_err(|e| {
            Error::Internal(format!(
                "Failed to parse search url '{}': {:?}",
                self.search_url,
                e
            ))
        })?;
        u.set_query(Some(&format!("q={}", query)));
        match meta.event_target.as_str() {
            "" => Ok(self.client.get(u.as_str())),
            _ => Ok(self.client.post(u.as_str()).form(&meta.as_map())),
        }
    }

    /// `search` returns a stream to receive pages of search results from
    /// the Microsoft Update Catalog. Calling `next` on the stream will return a `Result`
    /// containing either a `Vec<SearchResult>` or `None` if there are no more pages.
    ///
    /// # Parameters
    ///
    /// * `query` - The search query to use.
    ///
    /// # Example
    ///
    /// ```
    /// use msuc::prelude::*;
    /// use tokio_test;
    ///
    /// #[cfg(not(feature = "blocking"))]
    /// tokio_test::block_on(async {
    ///     let msuc_client = MsucClient::new().expect("Failed to create MSUC client");
    ///     let mut stream = msuc_client.search("MS08-067").expect("Failed to create search stream");
    ///     loop {
    ///         match stream.next().await {
    ///             Ok(Some(sr)) => {
    ///                 for r in sr {
    ///                     println!("{}: {}", r.id, r.title);
    ///                 }
    ///             }
    ///             Ok(None) => break,
    ///             Err(e) => {
    ///                 println!("Error: {:?}", e);
    ///             }
    ///         }
    ///     }
    /// });
    /// ```
    ///
    /// ```
    /// use msuc::prelude::*;
    /// use tokio_test;
    ///
    /// #[cfg(feature = "blocking")]
    /// {
    ///     let msuc_client = MsucClient::new().expect("Failed to create MSUC client");
    ///     let mut stream = msuc_client.search("MS08-067").expect("Failed to create search stream");
    ///     loop {
    ///         match stream.next() {
    ///             Ok(Some(sr)) => {
    ///                 for r in sr {
    ///                     println!("{}: {}", r.id, r.title);
    ///                 }
    ///             }
    ///             Ok(None) => break,
    ///             Err(e) => {
    ///                 println!("Error: {:?}", e);
    ///             }
    ///         }
    ///     }
    /// };
    /// ```
    pub fn search(&self, query: &str) -> Result<SearchResultsStream, Error> {
        SearchResultsStream::new(SearchPageMeta::default(), query)
    }

    /// `get_update` retrieves the update details for the given update id.
    /// The update id can be found in the `id` field of the `SearchResult` struct.
    ///
    /// # Parameters
    ///
    /// * `update_id` - The update id to retrieve details for.
    ///
    /// # Example
    ///
    /// ```
    /// use msuc::prelude::*;
    /// use tokio_test;
    ///
    /// #[cfg(not(feature = "blocking"))]
    /// tokio_test::block_on(async {
    ///     let msuc_client = MsucClient::new().expect("Failed to create MSUC client");
    ///    // MS08-067
    ///     msuc_client.get_update("9397a21f-246c-453b-ac05-65bf4fc6b68b").await.expect("Failed to get update details");
    /// });
    /// ```
    ///
    /// ```
    /// use msuc::prelude::*;
    ///
    /// #[cfg(feature = "blocking")]
    /// {
    ///     let msuc_client = MsucClient::new().expect("Failed to create MSUC client");
    ///     // MS08-067
    ///     msuc_client.get_update("9397a21f-246c-453b-ac05-65bf4fc6b68b").expect("Failed to get update details");
    /// }
    #[cfg(not(feature = "blocking"))]
    pub async fn get_update(&self, update_id: &str) -> Result<Update, Error> {
        let url = format!("{}{}", self.update_url, update_id);
        let resp = self
            .client
            .get(url.as_str())
            .send()
            .await
            .map_err(Error::Client)?;
        resp.error_for_status_ref()?;
        let html = resp.text().await.map_err(Error::Client)?;
        parse_update_details(&html).map_err(|e| {
            Error::Search(format!(
                "Failed to parse update details for {}: {:?}",
                update_id, e
            ))
        })
    }

    #[cfg(feature = "blocking")]
    pub fn get_update(&self, update_id: &str) -> Result<Update, Error> {
        let url = format!("{}{}", self.update_url, update_id);
        let resp = self
            .client
            .get(url.as_str())
            .send()
            .map_err(Error::Client)?;
        resp.error_for_status_ref()?;
        let html = resp.text().map_err(Error::Client)?;
        parse_update_details(&html).map_err(|e| {
            Error::Search(format!(
                "Failed to parse update details for {}: {:?}",
                update_id, e
            ))
        })
    }
}
