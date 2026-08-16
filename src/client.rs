use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, ACCEPT};
use serde_json::Value;
use tokio::time::sleep;

use crate::errors::PlateAPIError;
use crate::types::*;

const VALID_STATES: &[&str] = &["NSW", "VIC", "QLD", "SA", "WA", "TAS", "NT", "ACT"];

pub struct PlateAPI {
    api_key: String,
    base_url: String,
    client: reqwest::Client,
    max_retries: u32,
}

impl PlateAPI {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self::with_options(api_key, ClientOptions::default())
    }

    pub fn with_options(api_key: impl Into<String>, opts: ClientOptions) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(opts.timeout_secs))
            .build()
            .expect("failed to build HTTP client");

        Self {
            api_key: api_key.into(),
            base_url: opts.base_url.trim_end_matches('/').to_string(),
            client,
            max_retries: opts.max_retries,
        }
    }

    pub async fn lookup(
        &self,
        plate: &str,
        state: &str,
        detailed: bool,
    ) -> Result<LookupResult, PlateAPIError> {
        let s = state.trim().to_uppercase();
        let p = plate.trim().to_uppercase();

        if !VALID_STATES.contains(&s.as_str()) {
            return Err(PlateAPIError::InvalidArgument(format!(
                "invalid state '{}', must be one of: ACT, NSW, NT, QLD, SA, TAS, VIC, WA",
                s
            )));
        }
        if p.is_empty() {
            return Err(PlateAPIError::InvalidArgument(
                "plate cannot be empty".to_string(),
            ));
        }

        let mut params = vec![("plate", p.as_str()), ("state", s.as_str())];
        if detailed {
            params.push(("detailed", "true"));
        }

        let resp = self.request("/api/v1/lookup", &params, true).await?;

        let mut result: LookupResult =
            serde_json::from_value(resp.body).map_err(|e| PlateAPIError::Request(e.to_string()))?;

        if let Some(ref mut v) = result.vehicle {
            v.year = v.lowest_year;
        }
        for alt in &mut result.alternatives {
            alt.year = alt.lowest_year;
        }

        result.rate_limit = resp.rate_limit;
        Ok(result)
    }

    pub async fn vehicles(
        &self,
        opts: Option<&VehiclesOptions>,
    ) -> Result<VehiclesResult, PlateAPIError> {
        let mut params: Vec<(&str, String)> = Vec::new();
        if let Some(o) = opts {
            if let Some(ref v) = o.make {
                params.push(("make", v.clone()));
            }
            if let Some(ref v) = o.model {
                params.push(("model", v.clone()));
            }
            if let Some(v) = o.year {
                params.push(("year", v.to_string()));
            }
            if let Some(ref v) = o.series {
                params.push(("series", v.clone()));
            }
            if let Some(ref v) = o.engine {
                params.push(("engine", v.clone()));
            }
            if let Some(ref v) = o.variant {
                params.push(("variant", v.clone()));
            }
        }

        let str_params: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let resp = self.request("/api/v1/vehicles", &str_params, true).await?;

        let raw: Value =
            serde_json::from_value(resp.body).map_err(|e| PlateAPIError::Request(e.to_string()))?;

        let mut result = VehiclesResult {
            success: raw["success"].as_bool().unwrap_or(false),
            r#type: None,
            data: Vec::new(),
            total: raw["total"].as_i64().unwrap_or(0) as i32,
            duration_ms: raw["duration_ms"].as_f64(),
        };

        if let Some(arr) = raw["data"].as_array() {
            if let Some(first) = arr.first() {
                result.r#type = first["type"].as_str().map(|s| s.to_string());
                if let Some(data_arr) = first["data"].as_array() {
                    for item in data_arr {
                        if let Some(s) = item.as_str() {
                            result.data.push(s.to_string());
                        } else {
                            result.data.push(item.to_string());
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    pub async fn usage(&self) -> Result<Usage, PlateAPIError> {
        let resp = self
            .request::<&str>("/api/v1/keys/usage", &[], true)
            .await?;
        serde_json::from_value(resp.body).map_err(|e| PlateAPIError::Request(e.to_string()))
    }

    pub async fn logs(&self, opts: Option<&LogsOptions>) -> Result<LogsResult, PlateAPIError> {
        let limit = opts
            .and_then(|o| o.limit)
            .unwrap_or(100)
            .to_string();
        let offset = opts
            .and_then(|o| o.offset)
            .unwrap_or(0)
            .to_string();

        let mut params: Vec<(&str, String)> = vec![
            ("limit", limit),
            ("offset", offset),
        ];
        if let Some(o) = opts {
            if let Some(ref v) = o.since {
                params.push(("since", v.clone()));
            }
            if let Some(ref v) = o.until {
                params.push(("until", v.clone()));
            }
            if let Some(ref v) = o.plate {
                params.push(("plate", v.clone()));
            }
            if let Some(v) = o.success {
                params.push(("success", v.to_string()));
            }
        }

        let str_params: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let resp = self.request("/api/v1/keys/logs", &str_params, true).await?;
        serde_json::from_value(resp.body).map_err(|e| PlateAPIError::Request(e.to_string()))
    }

    pub async fn health(&self) -> Result<HealthStatus, PlateAPIError> {
        let resp = self
            .request::<&str>("/api/v1/health", &[], false)
            .await?;
        serde_json::from_value(resp.body).map_err(|e| PlateAPIError::Request(e.to_string()))
    }

    async fn request<V: AsRef<str>>(
        &self,
        path: &str,
        params: &[(&str, V)],
        auth: bool,
    ) -> Result<Response, PlateAPIError> {
        let url = format!("{}{}", self.base_url, path);

        let mut last_err = PlateAPIError::Request("request failed".to_string());

        for attempt in 0..=self.max_retries {
            let mut headers = HeaderMap::new();
            headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
            if auth {
                headers.insert(
                    "X-API-Key",
                    HeaderValue::from_str(&self.api_key)
                        .map_err(|e| PlateAPIError::Request(e.to_string()))?,
                );
            }

            let query: Vec<(&str, &str)> =
                params.iter().map(|(k, v)| (*k, v.as_ref())).collect();

            let result = self
                .client
                .get(&url)
                .headers(headers)
                .query(&query)
                .send()
                .await;

            let resp = match result {
                Ok(r) => r,
                Err(e) => {
                    last_err = PlateAPIError::Request(format!("request failed: {}", e));
                    if attempt < self.max_retries {
                        sleep(backoff_delay(attempt)).await;
                        continue;
                    }
                    return Err(last_err);
                }
            };

            let status = resp.status().as_u16();
            let retry_after_header = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<f64>().ok());

            let rate_limit = RateLimit {
                limit: resp
                    .headers()
                    .get("x-ratelimit-limit")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse().ok()),
                remaining: resp
                    .headers()
                    .get("x-ratelimit-remaining")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse().ok()),
                plan: resp
                    .headers()
                    .get("x-ratelimit-plan")
                    .and_then(|v| v.to_str().ok())
                    .map(|v| v.to_string()),
            };

            let body_bytes = resp
                .bytes()
                .await
                .map_err(|e| PlateAPIError::Request(format!("failed to read body: {}", e)))?;

            let body: Value = serde_json::from_slice(&body_bytes)
                .unwrap_or_else(|_| Value::Object(serde_json::Map::new()));

            if status == 401 {
                return Err(PlateAPIError::Authentication);
            }

            if status == 429 {
                let code = body["code"].as_str().unwrap_or("");
                if code == "quota_exceeded" {
                    return Err(PlateAPIError::QuotaExceeded);
                }
                if attempt < self.max_retries {
                    let wait = retry_after_header
                        .map(Duration::from_secs_f64)
                        .unwrap_or_else(|| backoff_delay(attempt));
                    sleep(wait).await;
                    continue;
                }
                return Err(PlateAPIError::RateLimit {
                    retry_after: retry_after_header,
                });
            }

            if status >= 500 {
                if attempt < self.max_retries {
                    let wait = retry_after_header
                        .map(Duration::from_secs_f64)
                        .unwrap_or_else(|| backoff_delay(attempt));
                    sleep(wait).await;
                    continue;
                }
                return Err(PlateAPIError::Server {
                    status,
                    retry_after: retry_after_header,
                });
            }

            if status >= 400 {
                let message = body["detail"]
                    .as_str()
                    .or_else(|| body["error"].as_str())
                    .unwrap_or("request failed")
                    .to_string();
                return Err(PlateAPIError::Api { message, status });
            }

            return Ok(Response { body, rate_limit });
        }

        Err(last_err)
    }
}

struct Response {
    body: Value,
    rate_limit: RateLimit,
}

fn backoff_delay(attempt: u32) -> Duration {
    let base = 2f64.powi(attempt as i32);
    let jitter = rand_jitter();
    Duration::from_secs_f64(base + jitter)
}

fn rand_jitter() -> f64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let s = RandomState::new();
    let mut hasher = s.build_hasher();
    hasher.write_u64(0);
    (hasher.finish() % 500) as f64 / 1000.0
}
