use serde_json::{json, Value};

use crate::testing::*;
use crate::*;

fn client(srv: &FakeServer) -> PlateAPI {
    client_with(srv, ClientOptions::default())
}

fn client_with(srv: &FakeServer, opts: ClientOptions) -> PlateAPI {
    PlateAPI::with_options(
        "pk_live_test",
        ClientOptions {
            base_url: srv.url.clone(),
            ..opts
        },
    )
}

fn sleeps(c: &PlateAPI) -> Vec<f64> {
    c.recorded_sleeps.lock().unwrap().clone()
}

fn hilux() -> Value {
    json!({"year_range": "2015-2020", "lowest_year": 2015, "highest_year": 2020, "make": "TOYOTA", "model": "HILUX",
           "description": "15~20 TOYOTA HILUX 2.8L DIESEL", "body": "UTE", "engine": "2.8L", "vehicle_id": 1380801})
}

fn corolla(id: Value) -> Value {
    json!({"year_range": "2003-2006", "lowest_year": 2003, "highest_year": 2006, "make": "TOYOTA", "model": "COROLLA",
           "description": "03~06 TOYOTA COROLLA 1.8L PETROL", "body": "HATCHBACK", "engine": "1.8L", "vehicle_id": id})
}

fn record() -> Value {
    json!({"vehicle_id": 1380801, "vehicle_type": "car", "make": "TOYOTA", "model": "HILUX SR",
           "year_range": "2015-2020", "lowest_year": 2015, "highest_year": 2020, "years": [2015, 2016, 2017, 2018, 2019, 2020],
           "description": "15~20 TOYOTA HILUX 2.8L DIESEL", "long_description": "TOYOTA HILUX 10/2015~04/2020",
           "series": "GUN126R -GUN126", "engine": "2.8L,  DIE 1GDFTV I4 16v DOHC I/C Turbo CRD {130kW}",
           "variant": "SR,  4D Ute -, 4WD MR0KA3CD  [THAILAND], AT/MT", "body": "UTE", "body_size": null,
           "doors": 4, "drive": "4WD", "transmission": "AT/MT", "detail": "full"})
}

fn found() -> Value {
    json!({"success": true, "vehicle": hilux()})
}

// ---- lookup ----

#[tokio::test]
async fn exact_lookup() {
    let srv = FakeServer::start(vec![ok_with(
        json!({"success": true, "vehicle": hilux(), "alternatives": [], "match": "exact", "source": "plateapi",
               "duration_ms": 1509.9, "request_id": "req_abc"}),
        &[
            ("X-RateLimit-Limit", "600"),
            ("X-RateLimit-Remaining", "599"),
            ("X-RateLimit-Plan", "starter"),
        ],
    )]);
    let c = client(&srv);
    let r = c.lookup("abc 123", "vic", false).await.unwrap();
    let call = &srv.calls()[0];
    assert_eq!(call.path, "/api/v1/lookup");
    assert_eq!(call.param("plate"), Some("ABC 123"));
    assert_eq!(call.param("state"), Some("VIC"));
    assert_eq!(call.header("X-API-Key"), Some("pk_live_test"));
    assert_eq!(
        call.header("User-Agent"),
        Some(format!("plateapi-rust/{}", VERSION).as_str())
    );
    assert!(r.success);
    assert_eq!(r.match_.as_deref(), Some("exact"));
    let v = r.vehicle.as_ref().unwrap();
    assert_eq!(v.vehicle_id, Some(1380801));
    assert_eq!(v.year, Some(2015));
    assert_eq!(r.candidate_ids(), vec![1380801]);
    assert_eq!(r.request_id.as_deref(), Some("req_abc"));
    assert_eq!(r.rate_limit.limit, Some(600));
    assert_eq!(r.rate_limit.remaining, Some(599));
    assert_eq!(r.rate_limit.plan.as_deref(), Some("starter"));
    assert_eq!(r.rate_limit.topup_remaining, None);
}

#[tokio::test]
async fn multiple_and_ambiguous() {
    let srv = FakeServer::start(vec![
        ok(
            json!({"success": true, "vehicle": corolla(json!(5152708)), "match": "multiple",
                  "alternatives": [corolla(json!(1857621)), corolla(json!(5152708))]}),
        ),
        ok(
            json!({"success": true, "vehicle": corolla(Value::Null), "match": "ambiguous",
                  "alternatives": [corolla(json!(5152708)), hilux()]}),
        ),
    ]);
    let c = client(&srv);
    let r = c.lookup("ABC123", "VIC", false).await.unwrap();
    assert_eq!(r.candidate_ids(), vec![5152708, 1857621]);
    let r = c.lookup("ABC123", "VIC", false).await.unwrap();
    assert_eq!(r.match_.as_deref(), Some("ambiguous"));
    assert_eq!(r.vehicle.as_ref().unwrap().vehicle_id, None);
    assert_eq!(r.candidate_ids(), vec![5152708, 1380801]);
}

#[tokio::test]
async fn not_found_sandbox_and_header_fallbacks() {
    let srv = FakeServer::start(vec![
        ok(
            json!({"success": false, "vehicle": null, "code": "not_found", "error": "No vehicle found", "request_id": "req_nf"}),
        ),
        ok_with(
            json!({"success": true, "vehicle": hilux(), "sandbox": true}),
            &[("X-Request-ID", "req_hdr"), ("X-Topup-Remaining", "17")],
        ),
        ok_with(found(), &[("X-RateLimit-Limit", "unlimited")]),
    ]);
    let c = client(&srv);
    let r = c.lookup("ZZZ999", "NSW", false).await.unwrap();
    assert!(!r.success);
    assert_eq!(r.code.as_deref(), Some("not_found"));
    assert!(r.vehicle.is_none());
    assert!(r.candidate_ids().is_empty());
    assert_eq!(r.match_, None);
    assert_eq!(r.request_id.as_deref(), Some("req_nf"));
    let r = c.lookup("TEST123", "VIC", true).await.unwrap();
    assert_eq!(srv.calls()[1].param("detailed"), Some("true"));
    assert!(r.sandbox);
    assert_eq!(r.request_id.as_deref(), Some("req_hdr"));
    assert_eq!(r.rate_limit.topup_remaining, Some(17));
    let r = c.lookup("ABC123", "VIC", false).await.unwrap();
    assert_eq!(r.rate_limit.limit, None);
}

#[tokio::test]
async fn internal_error_retried_then_returned() {
    let internal = || {
        ok(
            json!({"success": false, "vehicle": null, "code": "internal_error", "error": "Internal error"}),
        )
    };
    let srv = FakeServer::start(vec![internal(), ok(found())]);
    let c = client(&srv);
    assert!(c.lookup("ABC123", "VIC", false).await.unwrap().success);
    assert_eq!(srv.calls().len(), 2);
    assert_eq!(sleeps(&c).len(), 1);

    let srv = FakeServer::start(vec![internal(), internal(), internal(), internal()]);
    let c = client(&srv);
    let r = c.lookup("ABC123", "VIC", false).await.unwrap();
    assert_eq!(r.code.as_deref(), Some("internal_error"));
    assert_eq!(srv.calls().len(), 4);
}

#[tokio::test]
async fn lookup_validation_makes_no_request() {
    let srv = FakeServer::start(vec![]);
    let c = client(&srv);
    assert!(matches!(
        c.lookup("ABC123", "XX", false).await,
        Err(PlateAPIError::InvalidArgument(_))
    ));
    assert!(matches!(
        c.lookup("   ", "VIC", false).await,
        Err(PlateAPIError::InvalidArgument(_))
    ));
    assert!(srv.calls().is_empty());
}

// ---- errors ----

#[tokio::test]
async fn error_mapping_no_retry() {
    let cases: Vec<(u16, Option<Value>, &str)> = vec![
        (
            400,
            Some(json!({"detail": "Invalid state. Use one of: ACT, NSW"})),
            "Invalid state. Use one of: ACT, NSW",
        ),
        (
            401,
            Some(json!({"detail": "API key has been deactivated"})),
            "Invalid API key",
        ),
        (
            403,
            Some(json!({"detail": "This API key is not allowed from your IP address."})),
            "This API key is not allowed from your IP address.",
        ),
        (
            404,
            Some(
                json!({"success": false, "code": "not_found", "error": "No vehicle with that vehicle_id."}),
            ),
            "No vehicle with that vehicle_id.",
        ),
        (418, Some(json!({"detail": "teapot"})), "teapot"),
        (403, None, "Request failed (HTTP 403)"),
    ];
    for (status, body, expected) in cases {
        let srv = FakeServer::start(vec![res(status, body, &[("X-Request-ID", "req_x")])]);
        let c = client(&srv);
        let err = c.lookup("ABC123", "VIC", false).await.unwrap_err();
        assert_eq!(err.to_string(), expected, "status {}", status);
        assert_eq!(err.status_code(), Some(status));
        match (status, &err) {
            (400, PlateAPIError::BadRequest { request_id, .. }) => {
                assert_eq!(request_id.as_deref(), Some("req_x"))
            }
            (401, PlateAPIError::Authentication) => {}
            (403, PlateAPIError::PermissionDenied { .. }) => {
                assert_eq!(err.request_id(), Some("req_x"))
            }
            (404, PlateAPIError::NotFound { code, .. }) => {
                assert_eq!(code.as_deref(), Some("not_found"))
            }
            (418, PlateAPIError::Api { status: s, .. }) => assert_eq!(*s, 418),
            _ => panic!("status {}: wrong variant {:?}", status, err),
        }
        assert_eq!(srv.calls().len(), 1);
        assert!(sleeps(&c).is_empty());
    }
}

#[tokio::test]
async fn rate_limit_handling() {
    let srv = FakeServer::start(vec![
        res(
            429,
            Some(
                json!({"detail": "Per-minute rate limit exceeded (10/min on starter plan). Slow down."}),
            ),
            &[("Retry-After", "42")],
        ),
        ok(found()),
    ]);
    let c = client(&srv);
    assert!(c.lookup("ABC123", "VIC", false).await.unwrap().success);
    assert_eq!(sleeps(&c), vec![42.0]);
    assert_eq!(srv.calls().len(), 2);

    let srv = FakeServer::start(vec![res(
        429,
        Some(json!({"detail": "Vehicle query rate limit exceeded. Try again later."})),
        &[("Retry-After", "61")],
    )]);
    let c = client(&srv);
    let err = c
        .vehicles(Some(&VehiclesOptions {
            make: Some("TOYOTA".into()),
            ..Default::default()
        }))
        .await
        .unwrap_err();
    assert!(
        matches!(err, PlateAPIError::RateLimit { retry_after: Some(r) } if r == 61.0),
        "{:?}",
        err
    );
    assert!(err.is_rate_limited());
    assert!(sleeps(&c).is_empty());
    assert_eq!(srv.calls().len(), 1);

    let slow = || res(429, Some(json!({"detail": "slow down"})), &[]);
    let srv = FakeServer::start(vec![slow(), slow(), slow(), slow()]);
    let c = client(&srv);
    let err = c.lookup("ABC123", "VIC", false).await.unwrap_err();
    assert!(matches!(
        err,
        PlateAPIError::RateLimit { retry_after: None }
    ));
    assert_eq!(sleeps(&c).len(), 3);
    assert_eq!(srv.calls().len(), 4);

    let srv = FakeServer::start(vec![
        res(
            429,
            Some(json!({"detail": "slow down"})),
            &[("Retry-After", "3600")],
        ),
        ok(found()),
    ]);
    let c = client_with(
        &srv,
        ClientOptions {
            max_wait_secs: None,
            ..Default::default()
        },
    );
    assert!(c.lookup("ABC123", "VIC", false).await.unwrap().success);
    assert_eq!(sleeps(&c), vec![3600.0]);

    let srv = FakeServer::start(vec![
        res(
            429,
            Some(json!({"detail": "slow"})),
            &[("Retry-After", "Wed, 21 Oct 2026 07:28:00 GMT")],
        ),
        ok(found()),
    ]);
    let c = client(&srv);
    c.lookup("ABC123", "VIC", false).await.unwrap();
    let s = sleeps(&c);
    assert_eq!(s.len(), 1);
    assert!(s[0] >= 1.0 && s[0] < 2.0, "{:?}", s);
}

#[tokio::test]
async fn quota_exceeded_raises_at_once() {
    let srv = FakeServer::start(vec![res(
        429,
        Some(
            json!({"detail": "Monthly quota exceeded. Used 600/600 lookups on starter plan. Upgrade at plateapi.com.au"}),
        ),
        &[("Retry-After", "3600")],
    )]);
    let c = client(&srv);
    let err = c.lookup("ABC123", "VIC", false).await.unwrap_err();
    assert!(matches!(err, PlateAPIError::QuotaExceeded));
    assert!(err.is_rate_limited());
    assert_eq!(err.retry_after(), Some(3600.0));
    assert_eq!(err.code(), Some("quota_exceeded"));
    assert!(sleeps(&c).is_empty());
    assert_eq!(srv.calls().len(), 1);

    let srv = FakeServer::start(vec![res(
        429,
        Some(json!({"code": "quota_exceeded", "error": "Quota exceeded"})),
        &[],
    )]);
    let c = client(&srv);
    assert!(matches!(
        c.lookup("ABC123", "VIC", false).await,
        Err(PlateAPIError::QuotaExceeded)
    ));
}

#[tokio::test]
async fn server_errors() {
    let failed = || {
        res(
            503,
            Some(
                json!({"success": false, "vehicle": null, "code": "lookup_failed", "error": "Lookup temporarily unavailable. Please retry.", "request_id": "req_lf"}),
            ),
            &[],
        )
    };
    let srv = FakeServer::start(vec![failed(), failed(), failed(), failed()]);
    let c = client(&srv);
    let err = c.lookup("ABC123", "VIC", false).await.unwrap_err();
    assert!(
        matches!(
            err,
            PlateAPIError::Server {
                status: 503,
                retry_after: None
            }
        ),
        "{:?}",
        err
    );
    assert_eq!(srv.calls().len(), 4);
    assert_eq!(sleeps(&c).len(), 3);

    let srv = FakeServer::start(vec![
        res(
            503,
            Some(json!({"success": false, "code": "unavailable", "data": [], "total": 0})),
            &[("Retry-After", "5")],
        ),
        level("make", vec![json!("TOYOTA")]),
    ]);
    let c = client(&srv);
    assert_eq!(c.vehicles(None).await.unwrap().data, vec!["TOYOTA"]);
    assert_eq!(sleeps(&c), vec![5.0]);

    let srv = FakeServer::start(vec![
        res(502, None, &[]),
        res(502, None, &[]),
        res(502, None, &[]),
        res(502, None, &[]),
    ]);
    let c = client(&srv);
    let err = c.lookup("ABC123", "VIC", false).await.unwrap_err();
    assert!(matches!(err, PlateAPIError::Server { status: 502, .. }));
    assert_eq!(err.to_string(), "Server error (502)");
}

#[tokio::test]
async fn connection_errors_retried() {
    let srv = FakeServer::start(vec![drop_connection(), drop_connection(), ok(found())]);
    let c = client(&srv);
    assert!(c.lookup("ABC123", "VIC", false).await.unwrap().success);
    assert_eq!(sleeps(&c).len(), 2);

    let srv = FakeServer::start(vec![
        drop_connection(),
        drop_connection(),
        drop_connection(),
        drop_connection(),
    ]);
    let c = client(&srv);
    let err = c.lookup("ABC123", "VIC", false).await.unwrap_err();
    assert!(matches!(err, PlateAPIError::Request(_)), "{:?}", err);
    assert!(err.to_string().starts_with("Connection failed"), "{}", err);
    assert_eq!(err.status_code(), None);
    assert_eq!(srv.calls().len(), 4);
}

// ---- vehicles ----

#[tokio::test]
async fn vehicles_params_and_levels() {
    let srv = FakeServer::start(vec![
        level("engine", vec![json!("2.8L")]),
        level("make", vec![json!("ABARTH"), json!("TOYOTA")]),
        level("year", vec![json!(2021), json!(2020)]),
        level("make", vec![json!("YAMAHA")]),
        ok(json!({"success": true, "data": [], "total": 0})),
    ]);
    let c = client(&srv);
    let r = c
        .vehicles(Some(&VehiclesOptions {
            make: Some("TOYOTA".into()),
            model: Some("HILUX".into()),
            year: Some(2020),
            series: Some("".into()),
            ..Default::default()
        }))
        .await
        .unwrap();
    let call = &srv.calls()[0];
    assert_eq!(call.path, "/api/v1/vehicles");
    assert_eq!(call.param("make"), Some("TOYOTA"));
    assert_eq!(call.param("year"), Some("2020"));
    assert_eq!(call.param("series"), Some(""));
    assert!(r.success);
    assert_eq!(r.r#type.as_deref(), Some("engine"));
    assert_eq!(r.data, vec!["2.8L"]);
    assert_eq!(r.total, 1);
    assert!(r.vehicle_ids().is_empty());

    assert_eq!(
        c.vehicles(None).await.unwrap().r#type.as_deref(),
        Some("make")
    );
    assert!(srv.calls()[1].params.is_empty());
    let years = c
        .vehicles(Some(&VehiclesOptions {
            make: Some("TOYOTA".into()),
            model: Some("HILUX".into()),
            ..Default::default()
        }))
        .await
        .unwrap();
    assert_eq!(years.data, vec!["2021", "2020"]);
    c.vehicles(Some(&VehiclesOptions {
        vehicle_type: Some("motorcycle".into()),
        ..Default::default()
    }))
    .await
    .unwrap();
    assert_eq!(srv.calls()[3].param("type"), Some("motorcycle"));
    let empty = c
        .vehicles(Some(&VehiclesOptions {
            make: Some("NOPE".into()),
            ..Default::default()
        }))
        .await
        .unwrap();
    assert!(empty.data.is_empty() && empty.r#type.is_none());
    let err = c
        .vehicles(Some(&VehiclesOptions {
            vehicle_type: Some("truck".into()),
            ..Default::default()
        }))
        .await
        .unwrap_err();
    assert!(matches!(err, PlateAPIError::InvalidArgument(_)));
    assert_eq!(srv.calls().len(), 5);
}

#[tokio::test]
async fn vehicle_level_and_by_id() {
    let srv = FakeServer::start(vec![
        level(
            "vehicle",
            vec![
                json!({"vehicle_id": 5434441, "select_text": "a", "description": "d", "long_description": "l"}),
                json!({"vehicle_id": 2573335, "select_text": "b"}),
            ],
        ),
        ok(json!({"success": true, "vehicle": record(), "duration_ms": 2.2})),
        ok(
            json!({"success": true, "vehicle": {"vehicle_id": 7295503, "vehicle_type": "car", "make": "TOYOTA", "model": "X", "years": [], "detail": "basic", "series": null}}),
        ),
        res(
            404,
            Some(
                json!({"success": false, "code": "not_found", "error": "No vehicle with that vehicle_id."}),
            ),
            &[],
        ),
    ]);
    let c = client(&srv);
    let r = c
        .vehicles(Some(&VehiclesOptions {
            make: Some("KIA".into()),
            model: Some("SELTOS".into()),
            year: Some(2022),
            series: Some("EP81A".into()),
            engine: Some("2.0L".into()),
            variant: Some("x".into()),
            ..Default::default()
        }))
        .await
        .unwrap();
    assert_eq!(r.vehicle_ids(), vec![5434441, 2573335]);
    assert_eq!(r.vehicles[0].long_description.as_deref(), Some("l"));
    assert_eq!(r.vehicles[1].description, None);
    assert_eq!(r.data.len(), 2);

    let rec = c.vehicle_by_id(1380801).await.unwrap();
    assert_eq!(srv.calls()[1].path, "/api/v1/vehicles/1380801");
    assert!(srv.calls()[1].params.is_empty());
    assert_eq!(rec.vehicle_id, Some(1380801));
    assert_eq!(rec.vehicle_type.as_deref(), Some("car"));
    assert_eq!(rec.model.as_deref(), Some("HILUX SR"));
    assert_eq!(rec.years, vec![2015, 2016, 2017, 2018, 2019, 2020]);
    assert_eq!(rec.doors, Some(4));
    assert_eq!(rec.drive.as_deref(), Some("4WD"));
    assert_eq!(rec.body_size, None);
    assert_eq!(rec.detail.as_deref(), Some("full"));
    assert_eq!(rec.raw, record());

    let basic = c.vehicle_by_id(7295503).await.unwrap();
    assert_eq!(basic.detail.as_deref(), Some("basic"));
    assert!(basic.years.is_empty() && basic.series.is_none());

    let err = c.vehicle_by_id(1).await.unwrap_err();
    assert!(matches!(err, PlateAPIError::NotFound { .. }));
    assert_eq!(err.code(), Some("not_found"));
    assert!(matches!(
        c.vehicle_by_id(-1).await,
        Err(PlateAPIError::InvalidArgument(_))
    ));
    assert_eq!(srv.calls().len(), 4);
}

// ---- walker ----

fn vehicle(id: i64, text: &str) -> Value {
    json!({"vehicle_id": id, "select_text": text, "description": format!("desc {}", id), "long_description": format!("long {}", id)})
}

fn hilux_responses(trims: bool) -> Vec<FakeResponse> {
    let mut r = Vec::new();
    if trims {
        r.push(level(
            "model",
            vec![
                json!("HILUX"),
                json!("HILUX SR"),
                json!("HILUXX"),
                json!("CAMRY"),
            ],
        ));
    }
    r.extend([
        level("year", vec![json!(2020), json!(2019)]),
        level("series", vec![json!("GUN126R")]),
        level("engine", vec![json!("2.8L DIE")]),
        level("variant", vec![json!("4D Ute")]),
        level("vehicle", vec![vehicle(100, "4D Ute")]),
        level("series", vec![json!("GUN126R")]),
        level("engine", vec![json!("2.8L DIE")]),
        level("variant", vec![json!("4D Ute")]),
        level("vehicle", vec![vehicle(100, "4D Ute")]),
    ]);
    if trims {
        r.extend([
            level("year", vec![json!(2019)]),
            level("series", vec![json!("")]),
            level("engine", vec![json!("2.8L DIE"), json!("2.4L DIE")]),
            level("variant", vec![json!("SR 4D Ute")]),
            level(
                "vehicle",
                vec![vehicle(200, "SR 4D Ute"), vehicle(201, "SR 4D Ute")],
            ),
            level("variant", vec![json!("SR 4D Ute")]),
            level("vehicle", vec![vehicle(300, "SR 4D Ute")]),
        ]);
    }
    r
}

async fn collect(w: &mut VehicleWalker<'_>) -> Result<Vec<WalkedVehicle>, PlateAPIError> {
    let mut out = Vec::new();
    while let Some(v) = w.next().await? {
        out.push(v);
    }
    Ok(out)
}

fn ids(vs: &[WalkedVehicle]) -> Vec<i64> {
    vs.iter().map(|v| v.vehicle_id).collect()
}

#[tokio::test]
async fn walk_with_trims_dedupes_across_years() {
    let responses = hilux_responses(true);
    let n = responses.len();
    let srv = FakeServer::start(responses);
    let c = client(&srv);
    let mut w = c
        .walk_vehicles("TOYOTA", "HILUX", WalkOptions::default())
        .unwrap();
    let found = collect(&mut w).await.unwrap();
    assert_eq!(ids(&found), vec![100, 200, 201, 300]);
    let first = &found[0];
    assert_eq!(
        (
            first.make.as_str(),
            first.model.as_str(),
            first.year,
            first.series.as_str(),
            first.engine.as_str(),
            first.variant.as_str()
        ),
        ("TOYOTA", "HILUX", 2020, "GUN126R", "2.8L DIE", "4D Ute")
    );
    assert_eq!(first.description.as_deref(), Some("desc 100"));
    assert_eq!(first.vehicle_type, "car");
    assert_eq!(first.raw, vehicle(100, "4D Ute"));
    assert_eq!(found[1].model, "HILUX SR");
    assert_eq!(found[1].series, "");
    assert_eq!(found[3].engine, "2.4L DIE");
    let calls = srv.calls();
    assert_eq!(
        calls[0].params,
        vec![("make".to_string(), "TOYOTA".to_string())]
    );
    assert_eq!(calls[7].param("year"), Some("2019"));
    assert_eq!(calls[7].param("series"), Some("GUN126R"));
    assert_eq!(calls[11].param("series"), None);
    assert_eq!(calls[12].param("series"), Some(""));
    assert_eq!(calls.len(), n);
    assert_eq!(sleeps(&c), vec![0.15; n]);
    assert_eq!(w.known().len(), 4);
    assert!(w.known().contains(&WalkKey {
        vehicle_id: 300,
        year: None
    }));
}

#[tokio::test]
async fn walk_options() {
    let srv = FakeServer::start(hilux_responses(false));
    let c = client(&srv);
    let mut w = c
        .walk_vehicles(
            "TOYOTA",
            "HILUX",
            WalkOptions {
                trims: false,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(ids(&collect(&mut w).await.unwrap()), vec![100]);
    assert_eq!(srv.calls()[0].param("model"), Some("HILUX"));

    let srv = FakeServer::start(vec![
        level("year", vec![json!(2021), json!(2020), json!(2019)]),
        level("series", vec![json!("S")]),
        level("engine", vec![json!("2.8L DIE"), json!("2.4L DIE")]),
        level("variant", vec![json!("V")]),
        level("vehicle", vec![vehicle(1, "V")]),
    ]);
    let c = client(&srv);
    let mut w = c
        .walk_vehicles(
            "TOYOTA",
            "HILUX",
            WalkOptions {
                trims: false,
                year_from: Some(2020),
                year_to: Some(2020),
                engine_contains: Some("2.8l".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(ids(&collect(&mut w).await.unwrap()), vec![1]);
    assert!(srv.calls()[1..]
        .iter()
        .all(|c| c.param("year") == Some("2020")));
    assert_eq!(srv.calls()[3].param("engine"), Some("2.8L DIE"));

    let srv = FakeServer::start(hilux_responses(false));
    let c = client(&srv);
    let mut w = c
        .walk_vehicles(
            "TOYOTA",
            "HILUX",
            WalkOptions {
                trims: false,
                per_year: true,
                ..Default::default()
            },
        )
        .unwrap();
    let found = collect(&mut w).await.unwrap();
    assert_eq!(
        found
            .iter()
            .map(|v| (v.vehicle_id, v.year))
            .collect::<Vec<_>>(),
        vec![(100, 2020), (100, 2019)]
    );
    assert!(w.known().contains(&WalkKey {
        vehicle_id: 100,
        year: Some(2019)
    }));

    let srv = FakeServer::start(hilux_responses(true));
    let c = client(&srv);
    let known: std::collections::HashSet<WalkKey> = [100, 200]
        .iter()
        .map(|&id| WalkKey {
            vehicle_id: id,
            year: None,
        })
        .collect();
    let mut w = c
        .walk_vehicles(
            "TOYOTA",
            "HILUX",
            WalkOptions {
                known,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(ids(&collect(&mut w).await.unwrap()), vec![201, 300]);
    assert_eq!(w.into_known().len(), 4);

    let srv = FakeServer::start(vec![
        level("model", vec![json!("MT-07")]),
        level("year", vec![json!(2022)]),
        level("series", vec![json!("RM33")]),
        level("engine", vec![json!("0.7L")]),
        level("variant", vec![json!("MT-07")]),
        level("vehicle", vec![vehicle(7076553, "MT-07")]),
    ]);
    let c = client(&srv);
    let mut w = c
        .walk_vehicles(
            "YAMAHA",
            "MT-07",
            WalkOptions {
                vehicle_type: Some("motorcycle".into()),
                delay_ms: 0,
                ..Default::default()
            },
        )
        .unwrap();
    let found = collect(&mut w).await.unwrap();
    assert_eq!(found[0].vehicle_type, "motorcycle");
    assert!(srv
        .calls()
        .iter()
        .all(|c| c.param("type") == Some("motorcycle")));
    assert!(sleeps(&c).is_empty());
}

#[tokio::test]
async fn walk_errors_and_validation() {
    let mut responses = hilux_responses(false);
    responses[5] = res(
        429,
        Some(json!({"detail": "Vehicle query rate limit exceeded. Try again later."})),
        &[("Retry-After", "7200")],
    );
    let srv = FakeServer::start(responses);
    let c = client(&srv);
    let mut w = c
        .walk_vehicles(
            "TOYOTA",
            "HILUX",
            WalkOptions {
                trims: false,
                ..Default::default()
            },
        )
        .unwrap();
    let mut found = Vec::new();
    let err = loop {
        match w.next().await {
            Ok(Some(v)) => found.push(v.vehicle_id),
            Ok(None) => panic!("expected an error"),
            Err(e) => break e,
        }
    };
    assert!(matches!(err, PlateAPIError::RateLimit { retry_after: Some(r) } if r == 7200.0));
    assert_eq!(found, vec![100]);
    assert_eq!(w.known().len(), 1);

    let srv = FakeServer::start(vec![]);
    let c = client(&srv);
    for (make, model, opts) in [
        ("", "HILUX", WalkOptions::default()),
        ("TOYOTA", " ", WalkOptions::default()),
        (
            "TOYOTA",
            "HILUX",
            WalkOptions {
                year_from: Some(2020),
                year_to: Some(2019),
                ..Default::default()
            },
        ),
        (
            "TOYOTA",
            "HILUX",
            WalkOptions {
                vehicle_type: Some("bus".into()),
                ..Default::default()
            },
        ),
    ] {
        assert!(matches!(
            c.walk_vehicles(make, model, opts),
            Err(PlateAPIError::InvalidArgument(_))
        ));
    }
    assert!(srv.calls().is_empty());
}

// ---- account ----

#[tokio::test]
async fn usage_logs_and_health() {
    let srv = FakeServer::start(vec![
        ok(
            json!({"email": "a@b.c", "plan": "starter", "monthly_limit": 600, "used_this_month": 12, "remaining": 588, "percent_used": 2.0, "rate_limit_per_min": 10, "period_start": "2026-09-07 00:00:00", "topup_credits": 40}),
        ),
        ok(
            json!({"logs": [{"plate": "ABC123", "success": 1, "request_id": "req_1"}], "count": 1, "total": 9, "limit": 50, "offset": 10}),
        ),
        ok(json!({"status": "ok"})),
        res(503, Some(json!({"detail": "down"})), &[]),
    ]);
    let c = client(&srv);
    let u = c.usage().await.unwrap();
    assert_eq!(u.plan.as_deref(), Some("starter"));
    assert_eq!(u.monthly_limit, 600);
    assert_eq!(u.topup_credits, 40);
    assert_eq!(u.period_end, None);
    let logs = c
        .logs(Some(&LogsOptions {
            limit: Some(50),
            offset: Some(10),
            since: Some("2026-10-01T00:00:00".into()),
            plate: Some("ABC123".into()),
            success: Some(false),
            ..Default::default()
        }))
        .await
        .unwrap();
    let call = &srv.calls()[1];
    assert_eq!(call.param("limit"), Some("50"));
    assert_eq!(call.param("offset"), Some("10"));
    assert_eq!(call.param("since"), Some("2026-10-01T00:00:00"));
    assert_eq!(call.param("plate"), Some("ABC123"));
    assert_eq!(call.param("success"), Some("false"));
    assert_eq!(logs.logs[0].request_id.as_deref(), Some("req_1"));
    assert_eq!(logs.total, 9);
    let h = c.health().await.unwrap();
    assert_eq!(h.status, "ok");
    assert_eq!(srv.calls()[2].header("X-API-Key"), None);
    assert!(matches!(
        c.health().await,
        Err(PlateAPIError::Server { status: 503, .. })
    ));
    assert_eq!(srv.calls().len(), 4);
    assert!(sleeps(&c).is_empty());
}

#[tokio::test]
async fn quick_start_from_010_still_works() {
    let srv = FakeServer::start(vec![ok(
        json!({"success": true, "vehicle": {"make": "TOYOTA", "model": "HILUX", "lowest_year": 2015}, "alternatives": []}),
    )]);
    let c = client(&srv);
    let result = c.lookup("ABC123", "VIC", false).await.unwrap();
    if result.success {
        if let Some(ref v) = result.vehicle {
            assert_eq!(
                format!(
                    "{} {}",
                    v.make.as_deref().unwrap_or(""),
                    v.model.as_deref().unwrap_or("")
                ),
                "TOYOTA HILUX"
            );
            assert_eq!(v.year, Some(2015));
        }
    }
    assert!(result.alternatives.is_empty());
}
