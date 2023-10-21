use msuc::SearchResultsStream;

#[tokio::test]
async fn test_msuc_client_stream() {
    let client = msuc::Client::new();
    assert!(client.is_ok(), "Client creation failed");
    let client = client.unwrap();
    let s = client.get_search_iterator("ms08-067");
    assert!(s.is_ok(), "Failed to create search stream");
    let mut iterator = s.unwrap();
    loop {
        match iterator.stream().await {
            Ok(Some(sr)) => {
                println!("{}", iterator.too_many_results());
                for r in sr {
                    println!("{}: {}", r.id, r.title);
                }
            }
            Ok(None) => break,
            Err(e) => {
                println!("Error: {:?}", e);
            }
        }
    }
}
