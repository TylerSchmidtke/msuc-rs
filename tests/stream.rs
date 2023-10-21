use msuc::SearchResultsStreamer;

#[cfg(not(feature = "blocking"))]
#[tokio::test]
async fn test_msuc_client_search_stream() {
    // IDs associated with the search term "ms08-067" as of 2023-10-20.
    let result_ids = vec![
        "d81221ef-b903-4b69-ad87-e31780dc7fd4",
        "1e806ffe-574e-4ea5-b274-e22d63a5fbef",
        "55576edf-acc2-4219-9a6d-b034b19bb0e8",
        "755168cd-64ca-488d-9a68-1c20945c3bd7",
        "9602ca4a-80a7-4d73-94c3-0088fcb5bce3",
        "12c2f202-b094-43db-afe4-c57b38cc33c4",
        "5f5b6e72-fa4f-43ae-a874-7bac8f6b9148",
        "0b8bea98-be70-41f8-9861-e924dd7d0071",
        "b761be9b-cb93-4991-874c-6a0cc9dec179",
        "bae30328-dd13-421a-a4ab-b8465264d354",
        "511e7801-60f4-408a-9007-8b94b157bb15",
        "02754c95-2a6a-40af-8ef5-98a6199f8f42",
        "d8c6d72a-20ca-4b29-904b-8cd6fd2b1875",
        "9397a21f-246c-453b-ac05-65bf4fc6b68b",
        "e5df31a3-b8e5-4142-b643-8be79ad598f0",
    ];
    let client = msuc::Client::new();
    assert!(client.is_ok(), "Client creation failed");
    let client = client.unwrap();
    let stream = client.search("ms08-067");
    assert!(stream.is_ok(), "Failed to create search stream");
    let mut stream = stream.unwrap();
    let page = stream.next().await;
    assert!(page.is_ok(), "Expected the next page to be Ok");
    let page = page.unwrap();

    // The unit tests confirm that pages parse correct, just check the ids after the call
    // succeeds.
    match page {
        Some(sr) => {
            assert_eq!(sr.len(), result_ids.len(), "Expected the search results to same number of results as the test vector");
            for r in sr.iter() {
                assert!(result_ids.contains(&r.id.as_str()), "Expected update IDs to contain {}", r.id);
            }
        }
        None => {
            panic!("Expected the first page to contain search results");
        },
    }
}

#[cfg(feature = "blocking")]
#[test]
fn test_msuc_client_search_stream() {
    // IDs associated with the search term "ms08-067" as of 2023-10-20.
    let result_ids = vec![
        "d81221ef-b903-4b69-ad87-e31780dc7fd4",
        "1e806ffe-574e-4ea5-b274-e22d63a5fbef",
        "55576edf-acc2-4219-9a6d-b034b19bb0e8",
        "755168cd-64ca-488d-9a68-1c20945c3bd7",
        "9602ca4a-80a7-4d73-94c3-0088fcb5bce3",
        "12c2f202-b094-43db-afe4-c57b38cc33c4",
        "5f5b6e72-fa4f-43ae-a874-7bac8f6b9148",
        "0b8bea98-be70-41f8-9861-e924dd7d0071",
        "b761be9b-cb93-4991-874c-6a0cc9dec179",
        "bae30328-dd13-421a-a4ab-b8465264d354",
        "511e7801-60f4-408a-9007-8b94b157bb15",
        "02754c95-2a6a-40af-8ef5-98a6199f8f42",
        "d8c6d72a-20ca-4b29-904b-8cd6fd2b1875",
        "9397a21f-246c-453b-ac05-65bf4fc6b68b",
        "e5df31a3-b8e5-4142-b643-8be79ad598f0",
    ];
    let client = msuc::Client::new();
    assert!(client.is_ok(), "Client creation failed");
    let client = client.unwrap();
    let stream = client.search("ms08-067");
    assert!(stream.is_ok(), "Failed to create search stream");
    let mut stream = stream.unwrap();
    let page = stream.next();
    assert!(page.is_ok(), "Expected the next page to be Ok");
    let page = page.unwrap();

    // The unit tests confirm that pages parse correct, just check the ids after the call
    // succeeds.
    match page {
        Some(sr) => {
            assert_eq!(sr.len(), result_ids.len(), "Expected the search results to same number of results as the test vector");
            for r in sr.iter() {
                assert!(result_ids.contains(&r.id.as_str()), "Expected update IDs to contain {}", r.id);
            }
        }
        None => {
            panic!("Expected the first page to contain search results");
        },
    }
}

#[cfg(not(feature = "blocking"))]
#[tokio::test]
async fn test_msuc_client_search_stream_multiple_pages() {
    let client = msuc::Client::new();
    assert!(client.is_ok(), "Client creation failed");
    let client = client.unwrap();
    let stream = client.search("2023-04");
    assert!(stream.is_ok(), "Failed to create search stream");
    let mut stream = stream.unwrap();

    // temporary page_count until it's added to the stream metadata
    let mut page_count = 0;
    loop {
        let page = stream.next().await;
        assert!(page.is_ok(), "Expected the next page to be Ok");
        let page = page.unwrap();
        match page {
            Some(sr) => {
                page_count += 1;
                assert!(!sr.is_empty(), "Expected the search results to not be empty");
                assert!(!stream.too_many_results(), "Expected too_many_results to be false");
            }
            None => {
                break;
            },
        }
    }
    assert_eq!(page_count, 5, "Expected the search stream to have 4 pages");
}

#[cfg(feature = "blocking")]
#[test]
fn test_msuc_client_search_stream_multiple_pages() {
    let client = msuc::Client::new();
    assert!(client.is_ok(), "Client creation failed");
    let client = client.unwrap();
    let stream = client.search("2023-04");
    assert!(stream.is_ok(), "Failed to create search stream");
    let mut stream = stream.unwrap();

    // temporary page_count until it's added to the stream metadata
    let mut page_count = 0;
    loop {
        let page = stream.next();
        assert!(page.is_ok(), "Expected the next page to be Ok");
        let page = page.unwrap();
        match page {
            Some(sr) => {
                page_count += 1;
                assert!(!sr.is_empty(), "Expected the search results to not be empty");
                assert!(!stream.too_many_results(), "Expected too_many_results to be false");
            }
            None => {
                break;
            },
        }
    }
    assert_eq!(page_count, 5, "Expected the search stream to have 4 pages");
}

#[cfg(not(feature = "blocking"))]
#[tokio::test]
async fn test_msuc_client_search_stream_too_many_results() {
    let client = msuc::Client::new();
    assert!(client.is_ok(), "Client creation failed");
    let client = client.unwrap();
    let stream = client.search("cumulative");
    assert!(stream.is_ok(), "Failed to create search stream");

    // The first page will tell us if this broad search has too many results.
    let mut stream = stream.unwrap();
    let page = stream.next().await;
    assert!(page.is_ok(), "Expected the next page to be Ok");
    let page = page.unwrap();
    match page {
        Some(sr) => {
            assert!(!sr.is_empty(), "Expected the search results to not be empty");
            assert!(stream.too_many_results(), "Expected too_many_results to be true");
        }
        None => {
            panic!("Expected the first page to contain search results");
        },
    }
}

#[cfg(feature = "blocking")]
#[test]
fn test_msuc_client_search_stream_too_many_results() {
    let client = msuc::Client::new();
    assert!(client.is_ok(), "Client creation failed");
    let client = client.unwrap();
    let stream = client.search("cumulative");
    assert!(stream.is_ok(), "Failed to create search stream");

    // The first page will tell us if this broad search has too many results.
    let mut stream = stream.unwrap();
    let page = stream.next();
    assert!(page.is_ok(), "Expected the next page to be Ok");
    let page = page.unwrap();
    match page {
        Some(sr) => {
            assert!(!sr.is_empty(), "Expected the search results to not be empty");
            assert!(stream.too_many_results(), "Expected too_many_results to be true");
        }
        None => {
            panic!("Expected the first page to contain search results");
        },
    }
}
