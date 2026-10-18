# PlateAPI Rust SDK

Async Rust client for [PlateAPI](https://plateapi.com.au) -- Australian vehicle registration
plate lookup and the PlateAPI vehicle database.

## Install

Add to your `Cargo.toml`:

```toml
[dependencies]
plateapi = { git = "https://github.com/PlateAPI/plateapi-rust", tag = "v0.2.0" }
tokio = { version = "1", features = ["full"] }
```

Requires Rust 2021 edition. Uses reqwest (async) under the hood.

## Quick start

```rust
use plateapi::PlateAPI;

#[tokio::main]
async fn main() {
    let client = PlateAPI::new("pk_live_your_api_key");

    let result = client.lookup("ABC123", "VIC", false).await.unwrap();
    if result.success {
        if let Some(ref v) = result.vehicle {
            println!("{} {} {}", v.make.as_deref().unwrap_or(""), v.model.as_deref().unwrap_or(""),
                     v.year_range.as_deref().unwrap_or(""));
            println!("{:?}", v.vehicle_id);   // PlateAPI's stable ID for this variant
            println!("{:?}", result.match_);  // "exact", "multiple" or "ambiguous"
        }
    }
}
```

Get a free API key (20 lookups a month, no card) at [plateapi.com.au/register](https://plateapi.com.au/register/).
Full API reference: [plateapi.com.au/docs](https://plateapi.com.au/docs/).

## Plate lookup

```rust
let result = client.lookup("ABC123", "VIC", false).await?;
let v = result.vehicle.as_ref().unwrap();

result.success                    // true when a vehicle was found
result.match_                     // Some("exact" | "multiple" | "ambiguous"); None when not found
v.make                            // Some("TOYOTA")
v.model                           // Some("HILUX")
v.year_range                      // Some("2015-2020") ("2020-ON" for models still in production)
v.lowest_year                     // Some(2015)
v.highest_year                    // Some(2020) (None for an open range)
v.year                            // same as lowest_year
v.body                            // Some("UTE") (may be None)
v.engine                          // Some("2.8L") (may be None)
v.description                     // Some("15~20 TOYOTA HILUX 2.8L DIESEL")
v.vehicle_id                      // Some(1380801) (None when the variant is not certain)
result.alternatives               // other candidate variants, same fields, each with its own vehicle_id
result.candidate_ids()            // every non-null vehicle_id, primary first: [1380801]
result.request_id                 // Some("req_7f3a9c1b4e8d2f6a0c5e") -- quote this when contacting support
result.duration_ms                // Some(1509.96)
result.sandbox                    // true only for the TEST123 sandbox plate
```

Valid states: `NSW`, `VIC`, `QLD`, `SA`, `WA`, `TAS`, `NT`, `ACT`. Plate and state are
upper-cased and trimmed for you; an unknown state returns `PlateAPIError::InvalidArgument` before
any request.

### Not found

A plate that does not resolve is a normal result, not an error. It is still HTTP 200 and it
still counts against your monthly quota (typos are common, so validate input first).

```rust
let result = client.lookup("ZZZ999", "NSW", false).await?;
if !result.success {
    println!("{:?}", result.code);    // Some("not_found")
    println!("{:?}", result.error);   // human-readable message
}
```

### `vehicle_id` and `match_`

Every vehicle in a lookup carries `vehicle_id`: PlateAPI's ID for the exact variant. It is stable
(the same variant always returns the same ID) and it is the same ID the vehicle database uses, so
you can link your own records (products, SKUs, fitment rows) to it. It is `None` when the variant
could not be identified with certainty; an ID is never guessed.

`match_` (the JSON field `match`, renamed because `match` is a keyword) tells you how to read the
result:

| `match_` | Meaning | What to store |
|----------|---------|---------------|
| `exact` | one variant | `vehicle.vehicle_id` |
| `multiple` | several trims of one make and model, all on sale at the same time; `vehicle` is the first with its ID, the rest are in `alternatives` | all of `candidate_ids()`, or let the customer pick |
| `ambiguous` | candidates are different makes or models, or different generations; `vehicle.vehicle_id` is `None` and `alternatives` lists every candidate (including the one shown as `vehicle`) with its ID | `candidate_ids()` (a picker is best) |

```rust
let result = client.lookup("ABC123", "VIC", false).await?;
if result.match_.as_deref() == Some("exact") {
    save(result.vehicle.as_ref().unwrap().vehicle_id.unwrap());
} else {
    for alt in &result.alternatives {
        println!("{:?} {:?} {:?}", alt.vehicle_id, alt.description, alt.year_range);
    }
    save_all(result.candidate_ids());
}
```

With `multiple` the primary `vehicle_id` identifies the first candidate, not necessarily the
plate's own trim. Lookup text (`make`, `model`, `description`) can be worded differently from the
record the ID points to: integrate on `vehicle_id`, not on text.

### Detailed lookup

```rust
let result = client.lookup("ABC123", "NSW", true).await?;
println!("{:?}", result.vehicle.unwrap().detailed_description);
// "TOYOTA HILUX SR Auto or Manual AN120 AN130 05/2015~12/2020 4 Door UTE 4x4 DIESEL 2.8 litre, ..."
```

`detailed_description` may be `None` for some vehicles even when requested.

### Rate limit headers

Each lookup result carries the quota headers from the response:

```rust
result.rate_limit.limit            // monthly lookup allowance
result.rate_limit.remaining        // lookups left this period
result.rate_limit.plan             // Some("starter")
result.rate_limit.topup_remaining  // remaining top-up credits, only while they are being consumed
```

## Vehicle database

Starter plans and above. The database holds 23,000+ Australian-market car variants (225+ makes)
and 15,000+ motorcycles, scooters and ATVs (420+ makes). Queries do not consume lookup quota; they
have their own limit of 500 requests per minute.

### Cascade

`vehicles()` returns one level of the seven-level cascade. Each call narrows it: no options gives
the makes, `make` gives models, and so on down to full vehicle records with a `vehicle_id`.

```rust
use plateapi::VehiclesOptions;

let r = client.vehicles(None).await?;                                   // r.r#type == Some("make")
&r.data[..3];                                                            // ["ABARTH", "ACE EV", "ACURA"]

let r = client.vehicles(Some(&VehiclesOptions {
    make: Some("TOYOTA".into()), ..Default::default() })).await?;        // "model"
let r = client.vehicles(Some(&VehiclesOptions {
    make: Some("TOYOTA".into()), model: Some("HILUX".into()),
    ..Default::default() })).await?;                                     // "year"  (newest first)
let r = client.vehicles(Some(&VehiclesOptions {
    make: Some("TOYOTA".into()), model: Some("HILUX".into()), year: Some(2019),
    ..Default::default() })).await?;                                     // "series"
let r = client.vehicles(Some(&VehiclesOptions {
    make: Some("TOYOTA".into()), model: Some("HILUX".into()), year: Some(2019),
    series: Some("GUN126R -GUN126".into()),
    ..Default::default() })).await?;                                     // "engine"
let r = client.vehicles(Some(&VehiclesOptions {
    make: Some("TOYOTA".into()), model: Some("HILUX".into()), year: Some(2019),
    series: Some("GUN126R -GUN126".into()),
    engine: Some("2.8L,  DIE 1GDFTV I4 16v DOHC I/C Turbo CRD {130kW}".into()),
    ..Default::default() })).await?;                                     // "variant"
let r = client.vehicles(Some(&VehiclesOptions {
    make: Some("TOYOTA".into()), model: Some("HILUX".into()), year: Some(2019),
    series: Some("GUN126R -GUN126".into()),
    engine: Some("2.8L,  DIE 1GDFTV I4 16v DOHC I/C Turbo CRD {130kW}".into()),
    variant: Some("4D Ute -, 4WD MR0HA3CD  [THAILAND], AT/MT".into()),
    ..Default::default() })).await?;                                     // "vehicle"
r.vehicles        // [CascadeVehicle { vehicle_id: Some(1346446), select_text, description, long_description, raw }]
r.vehicle_ids()   // [1346446]
```

`VehiclesResult` fields: `success`, `r#type` (the level returned), `data` (each value as text:
names, years as digits, and the JSON of each record at the vehicle level), `vehicles` and
`vehicle_ids()` (filled at the vehicle level), `total`, `duration_ms`.

Rules worth knowing:

- Pass each value exactly as the previous level returned it. Values contain commas, double spaces,
  braces and brackets, and matching is case-sensitive. The SDK URL-encodes them for you.
- An empty-string series is a real value. Pass `series: Some("".into())` to continue below it.
- Unknown values are not errors: you get `success == true` with an empty `data` vector. A `503`
  (database briefly unavailable) is retried by the SDK and then returned as
  `PlateAPIError::Server`.
- Trims are filed as separate models (`HILUX`, `HILUX SR`, `HILUX SR5 EXTRA CAB`), so to cover a
  base model walk every model name that starts with it. `walk_vehicles()` below does this.
- The vehicle level is a list: a few paths return two records whose catalogue entries overlap in
  that year. Use each record's description and `vehicle_by_id()` to tell them apart.
- `vehicle_id` identifies the variant, not the year: a variant returns the same ID under every
  model year it was sold.

### Motorcycles

Motorcycles have their own cascade. Pass `vehicle_type: Some("motorcycle".into())` on every
level (`"car"` is the default).

```rust
let r = client.vehicles(Some(&VehiclesOptions {
    vehicle_type: Some("motorcycle".into()), ..Default::default() })).await?;       // 420+ makes
let r = client.vehicles(Some(&VehiclesOptions {
    make: Some("YAMAHA".into()), model: Some("MT-07".into()),
    vehicle_type: Some("motorcycle".into()), ..Default::default() })).await?;
```

A motorcycle plate lookup returns the same ID as the motorcycle cascade. Any other `vehicle_type`
returns `PlateAPIError::InvalidArgument` before any request.

### One vehicle by ID

```rust
let record = client.vehicle_by_id(1380801).await?;

record.vehicle_id        // Some(1380801)
record.vehicle_type      // Some("car") or Some("motorcycle")
record.make              // Some("TOYOTA")
record.model             // Some("HILUX SR")
record.year_range        // Some("2015-2020")
record.lowest_year       // Some(2015)
record.highest_year      // Some(2020) (None for an open range)
record.years             // [2015, 2016, 2017, 2018, 2019, 2020]
record.description       // Some("15~20 TOYOTA HILUX 2.8L DIESEL")
record.long_description  // Some("TOYOTA HILUX 10/2015~04/2020")
record.series            // Some("GUN126R -GUN126")
record.engine            // Some("2.8L,  DIE 1GDFTV I4 16v DOHC I/C Turbo CRD {130kW}")
record.variant           // Some("SR,  4D Ute -, 4WD MR0KA3CD  [THAILAND], AT/MT")
record.body              // Some("UTE")
record.body_size         // Some("COMPACT" / "MEDIUM" / "FULL-SIZE") for SUVs, else None
record.doors             // Some(4)
record.drive             // Some("4WD"), "AWD", "RWD", "FWD", "6X4", ...
record.transmission      // Some("AT"), "MT", "AT/MT", "CVT", "DCT", "AMT"
record.detail            // Some("full") or Some("basic")
record.raw               // serde_json::Value of the vehicle object exactly as returned
```

- `detail == "basic"` means the vehicle has an ID but its full record has not been added yet:
  `series`, `variant` and `long_description` are `None` and `years` is empty. The ID does not
  change when the full record is added.
- `body`, `body_size`, `doors`, `drive` and `transmission` are read from the variant name and are
  `None` when it does not carry them.
- A retired ID that was merged into another returns the current record, so `record.vehicle_id` can
  differ from the ID you asked for. Compare them if you care.
- An unknown ID returns `PlateAPIError::NotFound` (code `not_found`). A negative ID returns
  `PlateAPIError::InvalidArgument` before any request.
- The sandbox plate `TEST123` returns `vehicle_id` 1380801, a real record, so you can test this
  call without using quota.

## Walking the cascade

`walk_vehicles()` collects every `vehicle_id` for one make and model: it walks models, years,
series, engines, variants and vehicles for you and returns each vehicle the first time it is seen.
This is how you map existing fitment text (make, model, years, engine) to PlateAPI IDs once.

```rust
use plateapi::WalkOptions;

let mut walker = client.walk_vehicles("TOYOTA", "HILUX", WalkOptions {
    year_from: Some(2015), year_to: Some(2020), engine_contains: Some("2.8L".into()),
    ..Default::default()
})?;
while let Some(v) = walker.next().await? {
    println!("{} {} {} {} {} {}", v.vehicle_id, v.model, v.year, v.series, v.engine, v.variant);
}
```

| Option | Default | Meaning |
|--------|---------|---------|
| `make`, `model` (arguments) | required | base model; must be non-empty |
| `trims` | `true` | also walk every model name starting with `model + " "` (`HILUX SR`, `HILUX SR5 EXTRA CAB`). `false` walks `model` only and skips the models request |
| `year_from`, `year_to` | `None` | inclusive model-year window; `InvalidArgument` if from > to |
| `engine_contains` | `None` | only engines whose name contains this text (case-insensitive), e.g. `"2.8L"` or `"DIE"` |
| `vehicle_type` | `None` | `"motorcycle"` for the motorcycle cascade |
| `per_year` | `false` | return one row per vehicle per model year instead of one row per vehicle (for year-level master lists) |
| `known` | empty | a `HashSet<WalkKey>` of vehicles already handled; they are not returned again. The walker keeps it up to date: read it back with `walker.known()` or `walker.into_known()` to resume later. With `per_year` the key's `year` is set |
| `delay_ms` | `150` | milliseconds to pause after each request; the default stays under 500 requests/minute |

Each `WalkedVehicle` has `vehicle_id`, `make`, `model` (the exact model name it was found under),
`year`, `series`, `engine`, `variant` (the cascade's `select_text`), `description`,
`long_description`, `vehicle_type` and `raw` (the vehicle record as JSON). The path fields are
where the vehicle was first seen; with `per_year` you get a row for every year.

Results arrive as they are found, so you can stop at any time. Errors are returned at the point
they happen; everything already returned stays with you and `known()` is up to date. A `429`
whose `Retry-After` is longer than `max_wait_secs` (60 s by default) returns
`PlateAPIError::RateLimit` immediately rather than sleeping, so a long walk never sits silently.
Save your progress and resume later by passing the known set back in.

A Hilux walk (all trims, all years) is about 350 requests and takes a minute or two at the default
delay; a single make and model with a year window is usually a few seconds.

### Example: stream IDs to a CSV, resumable

```rust
use std::collections::HashSet;
use std::io::Write;

use plateapi::{PlateAPI, PlateAPIError, WalkKey, WalkOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let filename = "hilux.csv";
    let mut known = HashSet::new();
    if let Ok(text) = std::fs::read_to_string(filename) {
        for line in text.lines().skip(1) {
            if let Some(id) = line.split(',').next().and_then(|s| s.parse().ok()) {
                known.insert(WalkKey { vehicle_id: id, year: None });
            }
        }
    }

    let mut out = std::fs::OpenOptions::new().append(true).create(true).open(filename)?;
    if known.is_empty() {
        writeln!(out, "vehicle_id,make,model,year,series,engine,variant")?;
    }

    let client = PlateAPI::new(std::env::var("PLATEAPI_KEY")?);
    let mut walker = client.walk_vehicles("TOYOTA", "HILUX", WalkOptions { known, ..Default::default() })?;
    loop {
        match walker.next().await {
            Ok(Some(v)) => {
                writeln!(out, "{},{},{:?},{},{:?},{:?},{:?}", v.vehicle_id, v.make, v.model, v.year, v.series, v.engine, v.variant)?;
            }
            Ok(None) => break,
            Err(PlateAPIError::RateLimit { retry_after }) => {
                println!("rate limited, retry in {:?} s; {} rows saved", retry_after, walker.known().len());
                break;
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
```

Run it again later and it continues where it stopped.

### Linking products (SKUs) to vehicles

Keep a many-to-many table `(sku, vehicle_id)`. Walk the cascade once per fitment line to fill it,
then on each plate lookup query the table by the vehicle's `vehicle_id` (or every ID in
`candidate_ids()` when `match_` is `multiple`). Store IDs, not text. The full guide is at
[plateapi.com.au/docs/vehicles](https://plateapi.com.au/docs/vehicles/#vehicles-skus).

## Account

### Usage

```rust
let usage = client.usage().await?;
usage.plan               // Some("starter")
usage.monthly_limit      // 600
usage.used_this_month    // 12
usage.remaining          // 588
usage.percent_used       // Some(2.0)
usage.rate_limit_per_min // 10
usage.period_start       // Some("2026-09-07 00:00:00") (UTC)
usage.period_end         // None when there is no billing period (free plan)
usage.days_remaining     // None when there is no billing period
usage.last_lookup_at     // Some("2026-10-03 03:20:02")
usage.topup_credits      // remaining purchased top-up lookups
usage.cancel_at_period_end, usage.cancel_at
```

### Lookup history

```rust
use plateapi::LogsOptions;

let logs = client.logs(Some(&LogsOptions {
    limit: Some(50), offset: Some(0), since: Some("2026-10-01T00:00:00".into()),
    ..Default::default()
})).await?;
for entry in &logs.logs {
    println!("{:?} {:?} {:?} {} {:?} {:?}", entry.created_at, entry.plate, entry.state,
             entry.success, entry.duration_ms, entry.request_id);
}
println!("{} of {}", logs.count, logs.total);
```

`limit` is at most 500. `since` / `until` are ISO 8601 UTC. `success: Some(true)` / `false`
filters found / not found. `LogEntry` fields: `plate`, `state`, `success` (1 or 0), `error`,
`duration_ms`, `make`, `model`, `year`, `client_ip`, `request_id`, `created_at`.

### Health

```rust
client.health().await?.status   // "ok"
```

No key is sent, no quota is used, and the call makes a single attempt.

## Sandbox

Plate `TEST123` with any state returns a fixed vehicle instantly: free, no quota, no rate limit.
A valid key is still required. Use it in integration tests and CI.

```rust
let result = client.lookup("TEST123", "VIC", false).await?;
result.sandbox                               // true
result.vehicle.as_ref().unwrap().vehicle_id  // Some(1380801)
client.vehicle_by_id(1380801).await?.model   // Some("HILUX SR")
```

## Errors

Every error is a `PlateAPIError`. The accessors `status_code()`, `retry_after()`, `code()`,
`request_id()` and `is_rate_limited()` work on any variant.

| Variant | HTTP | When |
|---------|------|------|
| `BadRequest { message, request_id }` | 400 | invalid plate, state, cascade parameter or vehicle ID |
| `Authentication` | 401 | missing, invalid or deactivated key; email not verified |
| `PermissionDenied { message, request_id }` | 403 | IP not on the key's allowlist, account banned, or vehicle database on a Free / Lite plan |
| `NotFound { message, code, request_id }` | 404 | `vehicle_by_id()` with an unknown ID |
| `RateLimit { retry_after }` | 429 | per-minute limit; `retry_after` in seconds |
| `QuotaExceeded` | 429 | monthly quota used up (`retry_after()` is 3600; `is_rate_limited()` is true) |
| `Server { status, retry_after }` | 5xx | after retries |
| `Api { message, status }` | other 4xx | the API's own message |
| `Request(String)` | none | connection failures, timeouts, unreadable responses |
| `InvalidArgument(String)` | none | rejected before any request |

```rust
use plateapi::{PlateAPI, PlateAPIError};

match client.lookup("ABC123", "VIC", false).await {
    Ok(result) => { /* use result */ }
    Err(PlateAPIError::QuotaExceeded) => eprintln!("monthly quota used up"),
    Err(PlateAPIError::RateLimit { retry_after }) => eprintln!("slow down, retry in {:?} seconds", retry_after),
    Err(PlateAPIError::Authentication) => eprintln!("key problem"),
    Err(PlateAPIError::Server { status, .. }) => eprintln!("temporary failure, not billed ({})", status),
    Err(e) => eprintln!("error {:?}: {}", e.status_code(), e),
}
```

A lookup that cannot be completed (`503`, code `lookup_failed`) is never billed; retry it after a
few seconds. A `200` with `code: "internal_error"` is also not billed and is retried by the SDK
before it is returned.

## Retries and waiting

| Response | Behaviour |
|----------|-----------|
| connection error, timeout | retried up to `max_retries` with backoff (1 s, 2 s, 4 s plus jitter), then `Request` |
| 400 / 401 / 403 / 404 | returned at once, no retry |
| 429 monthly quota | `QuotaExceeded` at once, no sleep |
| 429 other | sleeps `Retry-After` (backoff if absent) and retries; if `Retry-After` is longer than `max_wait_secs` or retries are used up, `RateLimit` |
| 5xx | same rule, then `Server` |
| 200 lookup with `internal_error` | retried with backoff, then returned |

```rust
use plateapi::{PlateAPI, ClientOptions};

let client = PlateAPI::with_options("pk_live_your_api_key", ClientOptions {
    base_url: "https://api.plateapi.com.au".to_string(),  // default
    timeout_secs: 30,            // per request
    max_retries: 3,              // retries after the first attempt
    max_wait_secs: Some(60.0),   // longest Retry-After to sleep; None waits any length
});
```

Every request sends `User-Agent: plateapi-rust/<version>` (`plateapi::VERSION`).

## Upgrading from 0.1.0

New in 0.2.0: `vehicle_id`, `match_`, `candidate_ids()`, `vehicle_by_id()`, `walk_vehicles()`,
`vehicle_type: "motorcycle"`, `vehicles` / `vehicle_ids()` on cascade results,
`topup_remaining`, the `BadRequest`, `PermissionDenied` and `NotFound` error variants, the error
accessors, `max_wait_secs`.

Every existing error variant keeps its exact shape, so 0.1.0 `match` arms compile unchanged.
Things to check when upgrading:

- `VehiclesOptions` and `ClientOptions` gained a field each; struct literals that listed every
  field without `..Default::default()` need the new field or the spread.
- A monthly-quota `429` now returns `QuotaExceeded` immediately. 0.1.0 only recognised a body
  code the API does not send, so it slept for an hour and retried.
- A `Retry-After` longer than `max_wait_secs` (60 s) returns the error instead of sleeping. Pass
  `max_wait_secs: None` for the old behaviour.
- `health()` makes one attempt.
- 400, 403 and 404 return the new `BadRequest`, `PermissionDenied` and `NotFound` variants
  instead of `Api`.

See [CHANGELOG.md](CHANGELOG.md).

## Links

- [API documentation](https://plateapi.com.au/docs/)
- [Vehicle database and vehicle IDs](https://plateapi.com.au/docs/vehicles/)
- [OpenAPI spec](https://plateapi.com.au/openapi.json)
- [Pricing](https://plateapi.com.au/pricing/)
- [Dashboard](https://plateapi.com.au/dashboard/)
- [Status page](https://plateapi.com.au/status/)
- [Support](mailto:support@plateapi.com.au)
