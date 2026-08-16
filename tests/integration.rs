use plateapi::{ClientOptions, PlateAPI, PlateAPIError, LogsOptions, VehiclesOptions};

#[tokio::test]
async fn health_returns_ok() {
    let client = PlateAPI::new("dummy");
    let health = client.health().await.unwrap();
    assert_eq!(health.status, "ok");
}

#[tokio::test]
async fn lookup_invalid_state() {
    let client = PlateAPI::new("dummy");
    let err = client.lookup("ABC123", "XX", false).await.unwrap_err();
    match err {
        PlateAPIError::InvalidArgument(msg) => assert!(msg.contains("invalid state")),
        other => panic!("expected InvalidArgument, got {:?}", other),
    }
}

#[tokio::test]
async fn lookup_empty_plate() {
    let client = PlateAPI::new("dummy");
    let err = client.lookup("", "VIC", false).await.unwrap_err();
    match err {
        PlateAPIError::InvalidArgument(msg) => assert!(msg.contains("empty")),
        other => panic!("expected InvalidArgument, got {:?}", other),
    }
}

#[tokio::test]
async fn lookup_whitespace_plate() {
    let client = PlateAPI::new("dummy");
    let err = client.lookup("   ", "VIC", false).await.unwrap_err();
    match err {
        PlateAPIError::InvalidArgument(_) => {}
        other => panic!("expected InvalidArgument, got {:?}", other),
    }
}

#[tokio::test]
async fn state_case_insensitive() {
    let opts = ClientOptions {
        max_retries: 0,
        ..Default::default()
    };
    let client = PlateAPI::with_options("pk_live_invalid", opts);
    let err = client.lookup("TEST123", "vic", false).await.unwrap_err();
    match err {
        PlateAPIError::InvalidArgument(_) => {
            panic!("lowercase state was rejected -- case normalisation broken")
        }
        _ => {}
    }
}

#[tokio::test]
async fn all_states_accepted() {
    let opts = ClientOptions {
        max_retries: 0,
        ..Default::default()
    };
    let client = PlateAPI::with_options("pk_live_invalid", opts);
    for state in &["NSW", "VIC", "QLD", "SA", "WA", "TAS", "NT", "ACT"] {
        let err = client.lookup("TEST123", state, false).await.unwrap_err();
        if let PlateAPIError::InvalidArgument(_) = err {
            panic!("state {} was rejected", state);
        }
    }
}

#[tokio::test]
async fn lookup_bad_key() {
    let opts = ClientOptions {
        max_retries: 0,
        ..Default::default()
    };
    let client = PlateAPI::with_options("pk_live_invalid_key", opts);
    let err = client.lookup("TEST123", "VIC", false).await.unwrap_err();
    match err {
        PlateAPIError::Authentication | PlateAPIError::RateLimit { .. } => {}
        other => panic!("expected Authentication or RateLimit, got {:?}", other),
    }
}

#[tokio::test]
async fn usage_bad_key() {
    let opts = ClientOptions {
        max_retries: 0,
        ..Default::default()
    };
    let client = PlateAPI::with_options("pk_live_invalid_key", opts);
    let err = client.usage().await.unwrap_err();
    match err {
        PlateAPIError::Authentication | PlateAPIError::RateLimit { .. } => {}
        other => panic!("expected Authentication or RateLimit, got {:?}", other),
    }
}

#[tokio::test]
async fn logs_bad_key() {
    let opts = ClientOptions {
        max_retries: 0,
        ..Default::default()
    };
    let client = PlateAPI::with_options("pk_live_invalid_key", opts);
    let err = client.logs(None).await.unwrap_err();
    match err {
        PlateAPIError::Authentication | PlateAPIError::RateLimit { .. } => {}
        other => panic!("expected Authentication or RateLimit, got {:?}", other),
    }
}

#[tokio::test]
async fn vehicles_bad_key() {
    let opts = ClientOptions {
        max_retries: 0,
        ..Default::default()
    };
    let client = PlateAPI::with_options("pk_live_invalid_key", opts);
    let err = client.vehicles(None).await.unwrap_err();
    match err {
        PlateAPIError::Authentication | PlateAPIError::RateLimit { .. } => {}
        other => panic!("expected Authentication or RateLimit, got {:?}", other),
    }
}

#[tokio::test]
async fn connection_error() {
    let opts = ClientOptions {
        base_url: "http://127.0.0.1:19".to_string(),
        timeout_secs: 2,
        max_retries: 0,
    };
    let client = PlateAPI::with_options("dummy", opts);
    let err = client.health().await.unwrap_err();
    match err {
        PlateAPIError::Request(_) => {}
        other => panic!("expected Request error, got {:?}", other),
    }
}

#[tokio::test]
async fn custom_options() {
    let opts = ClientOptions {
        timeout_secs: 10,
        max_retries: 1,
        ..Default::default()
    };
    let client = PlateAPI::with_options("dummy", opts);
    let health = client.health().await.unwrap();
    assert_eq!(health.status, "ok");
}

#[tokio::test]
async fn trailing_slash_stripped() {
    let opts = ClientOptions {
        base_url: "https://api.plateapi.com.au///".to_string(),
        ..Default::default()
    };
    let client = PlateAPI::with_options("dummy", opts);
    let health = client.health().await.unwrap();
    assert_eq!(health.status, "ok");
}
