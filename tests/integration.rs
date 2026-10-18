//! Public-API checks that need no network, plus live checks that are ignored by default
//! (run them with `PLATEAPI_KEY=pk_live_... cargo test -- --ignored`).

use plateapi::{ClientOptions, PlateAPI, PlateAPIError, VehiclesOptions, WalkOptions};

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
    let err = client.lookup("   ", "VIC", false).await.unwrap_err();
    assert!(matches!(err, PlateAPIError::InvalidArgument(_)));
}

#[tokio::test]
async fn bad_vehicle_type_and_walk_options_are_rejected_before_any_request() {
    let client = PlateAPI::with_options(
        "dummy",
        ClientOptions {
            base_url: "http://127.0.0.1:1".to_string(),
            ..Default::default()
        },
    );
    let opts = VehiclesOptions {
        vehicle_type: Some("truck".into()),
        ..Default::default()
    };
    assert!(matches!(
        client.vehicles(Some(&opts)).await,
        Err(PlateAPIError::InvalidArgument(_))
    ));
    assert!(matches!(
        client.vehicle_by_id(-1).await,
        Err(PlateAPIError::InvalidArgument(_))
    ));
    assert!(matches!(
        client.walk_vehicles("", "HILUX", WalkOptions::default()),
        Err(PlateAPIError::InvalidArgument(_))
    ));
}

#[tokio::test]
async fn connection_error() {
    let opts = ClientOptions {
        base_url: "http://127.0.0.1:1".to_string(),
        timeout_secs: 2,
        max_retries: 0,
        ..Default::default()
    };
    let client = PlateAPI::with_options("dummy", opts);
    let err = client.health().await.unwrap_err();
    assert!(matches!(err, PlateAPIError::Request(_)), "{:?}", err);
    assert_eq!(err.status_code(), None);
}

#[tokio::test]
#[ignore]
async fn live_health_and_sandbox() {
    let key = std::env::var("PLATEAPI_KEY").expect("PLATEAPI_KEY");
    let client = PlateAPI::new(key);
    assert_eq!(client.health().await.unwrap().status, "ok");
    let result = client.lookup("TEST123", "VIC", false).await.unwrap();
    assert!(result.sandbox);
    assert_eq!(result.vehicle.unwrap().vehicle_id, Some(1380801));
}
