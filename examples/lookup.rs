use plateapi::{PlateAPI, PlateAPIError};

#[tokio::main]
async fn main() {
    let api_key = std::env::var("PLATEAPI_KEY").unwrap_or_else(|_| "pk_live_your_api_key".into());
    let client = PlateAPI::new(&api_key);

    match client.health().await {
        Ok(health) => println!("API status: {}", health.status),
        Err(e) => {
            eprintln!("Health check failed: {}", e);
            std::process::exit(1);
        }
    }

    match client.lookup("TEST123", "VIC", false).await {
        Ok(result) => {
            if result.success {
                if let Some(ref v) = result.vehicle {
                    println!("Make: {}", v.make.as_deref().unwrap_or(""));
                    println!("Model: {}", v.model.as_deref().unwrap_or(""));
                    println!("Year range: {}", v.year_range.as_deref().unwrap_or(""));
                    println!("Body: {}", v.body.as_deref().unwrap_or(""));
                    println!("Engine: {}", v.engine.as_deref().unwrap_or(""));
                    if let Some(ms) = result.duration_ms {
                        println!("Duration: {:.1}ms", ms);
                    }
                    println!("Request ID: {}", result.request_id.as_deref().unwrap_or(""));
                }
            }
            if let Some(remaining) = result.rate_limit.remaining {
                println!("Lookups remaining: {}", remaining);
            }
        }
        Err(PlateAPIError::Authentication) => {
            eprintln!("Invalid API key");
            std::process::exit(1);
        }
        Err(PlateAPIError::QuotaExceeded) => {
            eprintln!("Monthly quota exceeded");
            std::process::exit(1);
        }
        Err(PlateAPIError::RateLimit { retry_after }) => {
            eprint!("Rate limited");
            if let Some(secs) = retry_after {
                eprint!(", retry after {:.0}s", secs);
            }
            eprintln!();
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}
