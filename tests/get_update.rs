use msuc::prelude::*;

#[cfg(not(feature = "blocking"))]
#[tokio::test]
async fn test_get_update() {
    let client = MsucClient::new().expect("failed to create client");
    // MS08-067: KB958644
    let details = client.get_update("9602ca4a-80a7-4d73-94c3-0088fcb5bce3").await;
    assert!(details.is_ok(), "expected get_update call to succeed");
    let details = details.unwrap();
    assert_eq!(details.title, "Security Update for Windows XP x64 Edition (KB958644)");
    assert_eq!(details.id, "9602ca4a-80a7-4d73-94c3-0088fcb5bce3");
}

#[cfg(feature = "blocking")]
#[test]
fn test_get_update() {
    let client = MsucClient::new().expect("failed to create client");
    // MS08-067: KB958644
    let details = client.get_update("9602ca4a-80a7-4d73-94c3-0088fcb5bce3");
    assert!(details.is_ok(), "expected get_update call to succeed");
    let details = details.unwrap();
    assert_eq!(details.title, "Security Update for Windows XP x64 Edition (KB958644)");
    assert_eq!(details.id, "9602ca4a-80a7-4d73-94c3-0088fcb5bce3");
}