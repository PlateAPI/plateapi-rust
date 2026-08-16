# PlateAPI Rust SDK

Async Rust client for [PlateAPI](https://plateapi.com.au) -- Australian vehicle registration plate lookup.

## Install

Add to your `Cargo.toml`:

```toml
[dependencies]
plateapi = { git = "https://github.com/PlateAPI/plateapi-rust" }
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
            println!("{} {}", v.make.as_deref().unwrap_or(""), v.model.as_deref().unwrap_or(""));
        }
    }
}
```

## Plate lookup

```rust
let result = client.lookup("ABC123", "VIC", false).await?;

println!("{}", result.success);                                    // true
println!("{}", result.vehicle.as_ref().unwrap().make.as_deref().unwrap_or(""));       // "TOYOTA"
println!("{}", result.vehicle.as_ref().unwrap().model.as_deref().unwrap_or(""));      // "HILUX"
println!("{}", result.vehicle.as_ref().unwrap().lowest_year.unwrap_or(0));            // 2015
println!("{}", result.vehicle.as_ref().unwrap().highest_year.unwrap_or(0));           // 2023
println!("{}", result.vehicle.as_ref().unwrap().year_range.as_deref().unwrap_or("")); // "2015 - 2023"
println!("{}", result.vehicle.as_ref().unwrap().body.as_deref().unwrap_or(""));       // "UTILITY"
println!("{}", result.vehicle.as_ref().unwrap().engine.as_deref().unwrap_or(""));     // "2.8L"
println!("{}", result.duration_ms.unwrap_or(0.0));                 // 2451.3
println!("{}", result.source.as_deref().unwrap_or(""));            // "plateapi"
println!("{}", result.request_id.as_deref().unwrap_or(""));        // "req_7f3a9c1b4e..."
```

Valid states: `NSW`, `VIC`, `QLD`, `SA`, `WA`, `TAS`, `NT`, `ACT`.

## Detailed lookup

```rust
let result = client.lookup("ABC123", "NSW", true).await?;
if let Some(ref v) = result.vehicle {
    println!("{}", v.detailed_description.as_deref().unwrap_or(""));
    println!("{}", v.series.as_deref().unwrap_or(""));
}
```

## Multiple matches

```rust
for alt in &result.alternatives {
    println!("Also matched: {} {} ({})",
        alt.make.as_deref().unwrap_or(""),
        alt.model.as_deref().unwrap_or(""),
        alt.year_range.as_deref().unwrap_or(""));
}
```

## Vehicle database

Browse the full vehicle database (32,000+ vehicles, 213 makes). Each call narrows the cascade through all 7 levels. Paid plans only, no quota consumed.

```rust
use plateapi::VehiclesOptions;

// Step 1: All makes
let makes = client.vehicles(None).await?;
// makes.r#type == Some("make"), makes.data == ["ABARTH", "AC", ...]

// Step 2: Models for a make
let models = client.vehicles(Some(&VehiclesOptions {
    make: Some("TOYOTA".into()),
    ..Default::default()
})).await?;

// Step 3: Years
let years = client.vehicles(Some(&VehiclesOptions {
    make: Some("TOYOTA".into()),
    model: Some("HILUX".into()),
    ..Default::default()
})).await?;

// Step 4: Series
let series = client.vehicles(Some(&VehiclesOptions {
    make: Some("TOYOTA".into()),
    model: Some("HILUX".into()),
    year: Some(2020),
    ..Default::default()
})).await?;

// Step 5: Engines
let engines = client.vehicles(Some(&VehiclesOptions {
    make: Some("TOYOTA".into()),
    model: Some("HILUX".into()),
    year: Some(2020),
    series: Some("SR5".into()),
    ..Default::default()
})).await?;

// Step 6: Variants
let variants = client.vehicles(Some(&VehiclesOptions {
    make: Some("TOYOTA".into()),
    model: Some("HILUX".into()),
    year: Some(2020),
    series: Some("SR5".into()),
    engine: Some("2.8L".into()),
    ..Default::default()
})).await?;

// Step 7: Full vehicle details
let vehicles = client.vehicles(Some(&VehiclesOptions {
    make: Some("TOYOTA".into()),
    model: Some("HILUX".into()),
    year: Some(2020),
    series: Some("SR5".into()),
    engine: Some("2.8L".into()),
    variant: Some("4x4 Double Cab".into()),
})).await?;
```

For vehicles without a series code, pass an empty string:

```rust
let result = client.vehicles(Some(&VehiclesOptions {
    make: Some("TOYOTA".into()),
    model: Some("HILUX".into()),
    year: Some(2020),
    series: Some("".into()),
    ..Default::default()
})).await?;
```

## Check usage

```rust
let usage = client.usage().await?;
println!("{}/{} lookups used", usage.used_this_month, usage.monthly_limit);
println!("{} remaining", usage.remaining);
println!("{:.1}% used", usage.percent_used.unwrap_or(0.0));
println!("Plan: {}", usage.plan.as_deref().unwrap_or(""));
println!("Rate limit: {}/min", usage.rate_limit_per_min);
println!("Period: {} to {}",
    usage.period_start.as_deref().unwrap_or(""),
    usage.period_end.as_deref().unwrap_or(""));
println!("Days remaining: {}", usage.days_remaining.unwrap_or(0));
println!("Top-up credits: {}", usage.topup_credits);
```

## Request logs

```rust
use plateapi::LogsOptions;

// Last 10 lookups
let logs = client.logs(Some(&LogsOptions {
    limit: Some(10),
    ..Default::default()
})).await?;

for entry in &logs.logs {
    println!("{} | {} ({}) | {} | {:.0}ms",
        entry.created_at.as_deref().unwrap_or(""),
        entry.plate.as_deref().unwrap_or(""),
        entry.state.as_deref().unwrap_or(""),
        if entry.success != 0 { "found" } else { "not found" },
        entry.duration_ms.unwrap_or(0.0));
}
println!("Showing {} of {} total", logs.count, logs.total);
```

### Filtering

```rust
// Filter by plate
let plate_logs = client.logs(Some(&LogsOptions {
    plate: Some("ABC123".into()),
    ..Default::default()
})).await?;

// Only failed lookups
let failed = client.logs(Some(&LogsOptions {
    success: Some(false),
    ..Default::default()
})).await?;

// Time range
let july = client.logs(Some(&LogsOptions {
    since: Some("2026-07-01T00:00:00".into()),
    until: Some("2026-07-31T23:59:59".into()),
    ..Default::default()
})).await?;

// Pagination
let page1 = client.logs(Some(&LogsOptions {
    limit: Some(50),
    offset: Some(0),
    ..Default::default()
})).await?;
```

## Health check

No authentication required, no quota consumed.

```rust
let health = client.health().await?;
println!("{}", health.status); // "ok"
```

## Rate limits

```rust
let result = client.lookup("ABC123", "VIC", false).await?;
if let Some(remaining) = result.rate_limit.remaining {
    println!("Lookups remaining: {}", remaining);
}
```

## Error handling

```rust
use plateapi::{PlateAPI, PlateAPIError};

match client.lookup("ABC123", "VIC", false).await {
    Ok(result) => { /* use result */ }
    Err(PlateAPIError::Authentication) => {
        eprintln!("Invalid API key");
    }
    Err(PlateAPIError::QuotaExceeded) => {
        eprintln!("Monthly quota exceeded");
    }
    Err(PlateAPIError::RateLimit { retry_after }) => {
        eprint!("Rate limited");
        if let Some(secs) = retry_after {
            eprint!(", retry after {:.0}s", secs);
        }
        eprintln!();
    }
    Err(PlateAPIError::Server { status, .. }) => {
        eprintln!("Server error ({})", status);
    }
    Err(PlateAPIError::Api { message, status }) => {
        eprintln!("API error: {} (status {})", message, status);
    }
    Err(e) => {
        eprintln!("Error: {}", e);
    }
}
```

## Retry behaviour

The SDK automatically retries on:
- Connection errors
- Timeouts
- 429 rate limit responses (waits for Retry-After header)
- 5xx server errors

Default: 3 retries with exponential backoff and jitter. Configure with:

```rust
use plateapi::{PlateAPI, ClientOptions};

let client = PlateAPI::with_options("pk_live_your_api_key", ClientOptions {
    base_url: "https://api.plateapi.com.au".to_string(),
    timeout_secs: 60,
    max_retries: 5,
});
```

## Sandbox

Use plate `TEST123` with any state for testing. Returns a fixed response instantly, no quota consumed.

```rust
let result = client.lookup("TEST123", "VIC", false).await?;
// result.sandbox == true
// result.success == true
// result.vehicle.unwrap().make == Some("TOYOTA".into())
```

## Links

- [API Documentation](https://plateapi.com.au/docs)
- [Pricing](https://plateapi.com.au/pricing)
- [Dashboard](https://plateapi.com.au/dashboard)
- [Sign up for free](https://plateapi.com.au/register)
- [Status page](https://plateapi.com.au/status)
- [GitHub](https://github.com/PlateAPI)
