use std::collections::{HashSet, VecDeque};
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, USER_AGENT};
use serde_json::Value;

use crate::errors::PlateAPIError;
use crate::types::*;

/// The SDK version, sent as `User-Agent: plateapi-rust/<VERSION>`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// The pause after each request in `walk_vehicles`, in milliseconds.
pub const DEFAULT_WALK_DELAY_MS: u64 = 150;

const VALID_STATES: &[&str] = &["NSW", "VIC", "QLD", "SA", "WA", "TAS", "NT", "ACT"];
const VALID_VEHICLE_TYPES: &[&str] = &["car", "motorcycle"];
const USER_AGENT_VALUE: &str = concat!("plateapi-rust/", env!("CARGO_PKG_VERSION"));

pub struct PlateAPI {
    api_key: String,
    base_url: String,
    client: reqwest::Client,
    max_retries: u32,
    max_wait: Option<f64>,
    backoff_base: f64,
    #[cfg(test)]
    pub(crate) recorded_sleeps: std::sync::Arc<std::sync::Mutex<Vec<f64>>>,
}

struct Response {
    body: Value,
    rate_limit: RateLimit,
    request_id: Option<String>,
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
            max_wait: opts.max_wait_secs,
            backoff_base: 1.0,
            #[cfg(test)]
            recorded_sleeps: Default::default(),
        }
    }

    // ---- lookups ----

    /// Resolve a plate. A plate that does not resolve is a normal result with `success == false`
    /// (and `code == Some("not_found")`), not an error.
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

        let mut attempt = 0u32;
        let (resp, mut result) = loop {
            let resp = self
                .request("/api/v1/lookup", &params, true, self.max_retries)
                .await?;
            let result: LookupResult = serde_json::from_value(resp.body.clone())
                .map_err(|e| PlateAPIError::Request(format!("failed to parse response: {}", e)))?;
            // internal_error is a 200 that is not billed; one more try usually succeeds.
            if !result.success
                && result.code.as_deref() == Some("internal_error")
                && attempt < self.max_retries
            {
                self.sleep_secs(self.backoff(attempt)).await;
                attempt += 1;
                continue;
            }
            break (resp, result);
        };

        if let Some(ref mut v) = result.vehicle {
            v.year = v.lowest_year;
        }
        for alt in &mut result.alternatives {
            alt.year = alt.lowest_year;
        }
        if result.request_id.is_none() {
            result.request_id = resp.request_id;
        }
        result.rate_limit = resp.rate_limit;
        Ok(result)
    }

    // ---- vehicle database ----

    /// One level of the vehicle database cascade.
    pub async fn vehicles(
        &self,
        opts: Option<&VehiclesOptions>,
    ) -> Result<VehiclesResult, PlateAPIError> {
        let mut params: Vec<(&str, String)> = Vec::new();
        if let Some(o) = opts {
            if let Some(ref t) = o.vehicle_type {
                if !VALID_VEHICLE_TYPES.contains(&t.as_str()) {
                    return Err(PlateAPIError::InvalidArgument(format!(
                        "invalid vehicle_type '{}', must be one of: car, motorcycle",
                        t
                    )));
                }
                params.push(("type", t.clone()));
            }
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
        let resp = self
            .request("/api/v1/vehicles", &str_params, true, self.max_retries)
            .await?;
        let raw = resp.body;

        let mut result = VehiclesResult {
            success: raw["success"].as_bool().unwrap_or(false),
            r#type: None,
            data: Vec::new(),
            vehicles: Vec::new(),
            total: raw["total"].as_i64().unwrap_or(0) as i32,
            duration_ms: raw["duration_ms"].as_f64(),
        };

        if let Some(first) = raw["data"].as_array().and_then(|arr| arr.first()) {
            result.r#type = first["type"].as_str().map(|s| s.to_string());
            if let Some(items) = first["data"].as_array() {
                for item in items {
                    if let Some(s) = item.as_str() {
                        result.data.push(s.to_string());
                    } else {
                        result.data.push(item.to_string());
                        if item.is_object() {
                            result.vehicles.push(cascade_vehicle(item));
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    /// One vehicle record. An unknown ID is `PlateAPIError::NotFound`.
    pub async fn vehicle_by_id(&self, vehicle_id: i64) -> Result<VehicleRecord, PlateAPIError> {
        if vehicle_id < 0 {
            return Err(PlateAPIError::InvalidArgument(
                "vehicle_id must be a non-negative integer".to_string(),
            ));
        }
        let path = format!("/api/v1/vehicles/{}", vehicle_id);
        let resp = self
            .request::<&str>(&path, &[], true, self.max_retries)
            .await?;
        let raw = resp.body.get("vehicle").cloned().unwrap_or(Value::Null);
        let mut record: VehicleRecord = if raw.is_object() {
            serde_json::from_value(raw.clone())
                .map_err(|e| PlateAPIError::Request(format!("failed to parse response: {}", e)))?
        } else {
            VehicleRecord::default()
        };
        record.raw = raw;
        Ok(record)
    }

    /// Walk the cascade for one make and model (and, by default, its trims). Call
    /// `next().await` on the returned walker until it yields `Ok(None)`.
    pub fn walk_vehicles(
        &self,
        make: &str,
        model: &str,
        opts: WalkOptions,
    ) -> Result<VehicleWalker<'_>, PlateAPIError> {
        if make.trim().is_empty() {
            return Err(PlateAPIError::InvalidArgument(
                "make must be a non-empty string".to_string(),
            ));
        }
        if model.trim().is_empty() {
            return Err(PlateAPIError::InvalidArgument(
                "model must be a non-empty string".to_string(),
            ));
        }
        if let (Some(from), Some(to)) = (opts.year_from, opts.year_to) {
            if from > to {
                return Err(PlateAPIError::InvalidArgument(
                    "year_from must not be greater than year_to".to_string(),
                ));
            }
        }
        if let Some(ref t) = opts.vehicle_type {
            if !VALID_VEHICLE_TYPES.contains(&t.as_str()) {
                return Err(PlateAPIError::InvalidArgument(format!(
                    "invalid vehicle_type '{}', must be one of: car, motorcycle",
                    t
                )));
            }
        }
        let stack = if opts.trims {
            vec![Frame::Models]
        } else {
            vec![Frame::Model(model.to_string())]
        };
        Ok(VehicleWalker {
            client: self,
            make: make.to_string(),
            model: model.to_string(),
            needle: opts.engine_contains.as_ref().map(|s| s.to_uppercase()),
            opts,
            stack,
            output: VecDeque::new(),
        })
    }

    // ---- account ----

    pub async fn usage(&self) -> Result<Usage, PlateAPIError> {
        let resp = self
            .request::<&str>("/api/v1/keys/usage", &[], true, self.max_retries)
            .await?;
        serde_json::from_value(resp.body).map_err(|e| PlateAPIError::Request(e.to_string()))
    }

    pub async fn logs(&self, opts: Option<&LogsOptions>) -> Result<LogsResult, PlateAPIError> {
        let limit = opts.and_then(|o| o.limit).unwrap_or(100).to_string();
        let offset = opts.and_then(|o| o.offset).unwrap_or(0).to_string();

        let mut params: Vec<(&str, String)> = vec![("limit", limit), ("offset", offset)];
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
        let resp = self
            .request("/api/v1/keys/logs", &str_params, true, self.max_retries)
            .await?;
        serde_json::from_value(resp.body).map_err(|e| PlateAPIError::Request(e.to_string()))
    }

    /// Check the API. No key is sent, no quota is used, and a single attempt is made.
    pub async fn health(&self) -> Result<HealthStatus, PlateAPIError> {
        let resp = self
            .request::<&str>("/api/v1/health", &[], false, 0)
            .await?;
        serde_json::from_value(resp.body).map_err(|e| PlateAPIError::Request(e.to_string()))
    }

    // ---- transport ----

    fn backoff(&self, attempt: u32) -> f64 {
        self.backoff_base * 2f64.powi(attempt as i32) + rand_jitter()
    }

    fn can_wait(&self, secs: f64) -> bool {
        match self.max_wait {
            Some(max) => secs <= max,
            None => true,
        }
    }

    #[cfg(not(test))]
    async fn sleep_secs(&self, secs: f64) {
        tokio::time::sleep(Duration::from_secs_f64(secs.max(0.0))).await;
    }

    #[cfg(test)]
    async fn sleep_secs(&self, secs: f64) {
        self.recorded_sleeps.lock().unwrap().push(secs);
    }

    async fn request<V: AsRef<str>>(
        &self,
        path: &str,
        params: &[(&str, V)],
        auth: bool,
        max_retries: u32,
    ) -> Result<Response, PlateAPIError> {
        let url = format!("{}{}", self.base_url, path);
        let mut attempt = 0u32;

        loop {
            let mut headers = HeaderMap::new();
            headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
            headers.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_VALUE));
            if auth {
                headers.insert(
                    "X-API-Key",
                    HeaderValue::from_str(&self.api_key)
                        .map_err(|e| PlateAPIError::Request(e.to_string()))?,
                );
            }

            let query: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_ref())).collect();

            let resp = match self
                .client
                .get(&url)
                .headers(headers)
                .query(&query)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    if attempt < max_retries {
                        self.sleep_secs(self.backoff(attempt)).await;
                        attempt += 1;
                        continue;
                    }
                    let what = if e.is_timeout() {
                        "Request timed out"
                    } else {
                        "Connection failed"
                    };
                    return Err(PlateAPIError::Request(format!("{}: {}", what, e)));
                }
            };

            let status = resp.status().as_u16();
            let header = |name: &str| -> Option<String> {
                resp.headers()
                    .get(name)
                    .and_then(|v| v.to_str().ok())
                    .map(|v| v.to_string())
            };
            let retry_after = header("retry-after")
                .and_then(|v| v.trim().parse::<f64>().ok())
                .filter(|v| *v >= 0.0);
            let rate_limit = RateLimit {
                limit: header("x-ratelimit-limit").and_then(|v| v.parse().ok()),
                remaining: header("x-ratelimit-remaining").and_then(|v| v.parse().ok()),
                plan: header("x-ratelimit-plan"),
                topup_remaining: header("x-topup-remaining").and_then(|v| v.parse().ok()),
            };
            let header_request_id = header("x-request-id");

            let body_bytes = match resp.bytes().await {
                Ok(b) => b,
                Err(e) => {
                    if attempt < max_retries {
                        self.sleep_secs(self.backoff(attempt)).await;
                        attempt += 1;
                        continue;
                    }
                    return Err(PlateAPIError::Request(format!(
                        "failed to read body: {}",
                        e
                    )));
                }
            };
            let body: Value = serde_json::from_slice(&body_bytes).unwrap_or(Value::Null);
            let request_id = body["request_id"]
                .as_str()
                .map(|s| s.to_string())
                .or(header_request_id);

            if status < 400 {
                return Ok(Response {
                    body,
                    rate_limit,
                    request_id,
                });
            }

            let message = error_message(&body, status);
            let code = body["code"].as_str().map(|s| s.to_string());

            if status == 429 {
                if is_quota_exceeded(&body) {
                    return Err(PlateAPIError::QuotaExceeded);
                }
                let wait = retry_after.unwrap_or_else(|| self.backoff(attempt));
                if attempt < max_retries && self.can_wait(wait) {
                    self.sleep_secs(wait).await;
                    attempt += 1;
                    continue;
                }
                return Err(PlateAPIError::RateLimit { retry_after });
            }

            if status >= 500 {
                let wait = retry_after.unwrap_or_else(|| self.backoff(attempt));
                if attempt < max_retries && self.can_wait(wait) {
                    self.sleep_secs(wait).await;
                    attempt += 1;
                    continue;
                }
                return Err(PlateAPIError::Server {
                    status,
                    retry_after,
                });
            }

            return Err(match status {
                400 => PlateAPIError::BadRequest {
                    message,
                    request_id,
                },
                401 => PlateAPIError::Authentication,
                403 => PlateAPIError::PermissionDenied {
                    message,
                    request_id,
                },
                404 => PlateAPIError::NotFound {
                    message,
                    code: code.or_else(|| Some("not_found".to_string())),
                    request_id,
                },
                _ => PlateAPIError::Api { message, status },
            });
        }
    }
}

fn cascade_vehicle(item: &Value) -> CascadeVehicle {
    CascadeVehicle {
        vehicle_id: item["vehicle_id"].as_i64(),
        select_text: item["select_text"].as_str().map(|s| s.to_string()),
        description: item["description"].as_str().map(|s| s.to_string()),
        long_description: item["long_description"].as_str().map(|s| s.to_string()),
        raw: item.clone(),
    }
}

fn error_message(body: &Value, status: u16) -> String {
    for key in ["detail", "error"] {
        if let Some(s) = body[key].as_str() {
            if !s.is_empty() {
                return s.to_string();
            }
        }
    }
    format!("Request failed (HTTP {})", status)
}

fn is_quota_exceeded(body: &Value) -> bool {
    if body["code"].as_str() == Some("quota_exceeded") {
        return true;
    }
    body["detail"]
        .as_str()
        .map(|d| d.to_lowercase().starts_with("monthly quota exceeded"))
        .unwrap_or(false)
}

fn rand_jitter() -> f64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let s = RandomState::new();
    let mut hasher = s.build_hasher();
    hasher.write_u64(0);
    (hasher.finish() % 500) as f64 / 1000.0
}

// ---- walker ----

enum Frame {
    Models,
    Model(String),
    Year {
        model: String,
        year: i32,
    },
    Series {
        model: String,
        year: i32,
        series: String,
    },
    Engine {
        model: String,
        year: i32,
        series: String,
        engine: String,
    },
    Variant {
        model: String,
        year: i32,
        series: String,
        engine: String,
        variant: String,
    },
}

/// Walks the cascade for one make and model. Results arrive as they are found: call `next()`
/// until it returns `Ok(None)`. An error is returned at the point it happens; everything already
/// returned stays with you and `known()` is up to date, so you can resume later by passing it
/// back in `WalkOptions::known`.
pub struct VehicleWalker<'a> {
    client: &'a PlateAPI,
    make: String,
    model: String,
    needle: Option<String>,
    opts: WalkOptions,
    stack: Vec<Frame>,
    output: VecDeque<WalkedVehicle>,
}

impl<'a> VehicleWalker<'a> {
    /// The next vehicle, or `Ok(None)` when the walk is complete.
    pub async fn next(&mut self) -> Result<Option<WalkedVehicle>, PlateAPIError> {
        loop {
            if let Some(v) = self.output.pop_front() {
                return Ok(Some(v));
            }
            let frame = match self.stack.pop() {
                Some(f) => f,
                None => return Ok(None),
            };
            self.expand(frame).await?;
        }
    }

    /// Every vehicle delivered so far plus the known set passed in.
    pub fn known(&self) -> &HashSet<WalkKey> {
        &self.opts.known
    }

    /// Consume the walker and take the known set.
    pub fn into_known(self) -> HashSet<WalkKey> {
        self.opts.known
    }

    async fn fetch(&self, mut o: VehiclesOptions) -> Result<VehiclesResult, PlateAPIError> {
        o.vehicle_type = self.opts.vehicle_type.clone();
        let r = self.client.vehicles(Some(&o)).await?;
        if self.opts.delay_ms > 0 {
            self.client
                .sleep_secs(self.opts.delay_ms as f64 / 1000.0)
                .await;
        }
        Ok(r)
    }

    async fn expand(&mut self, frame: Frame) -> Result<(), PlateAPIError> {
        let make = self.make.clone();
        match frame {
            Frame::Models => {
                let r = self
                    .fetch(VehiclesOptions {
                        make: Some(make),
                        ..Default::default()
                    })
                    .await?;
                let prefix = format!("{} ", self.model);
                let models: Vec<String> = r
                    .data
                    .into_iter()
                    .filter(|m| *m == self.model || m.starts_with(&prefix))
                    .collect();
                for m in models.into_iter().rev() {
                    self.stack.push(Frame::Model(m));
                }
            }
            Frame::Model(model) => {
                let r = self
                    .fetch(VehiclesOptions {
                        make: Some(make),
                        model: Some(model.clone()),
                        ..Default::default()
                    })
                    .await?;
                let mut years: Vec<i32> = Vec::new();
                for y in r.data {
                    if let Ok(year) = y.trim().parse::<i32>() {
                        let below = matches!(self.opts.year_from, Some(from) if year < from);
                        let above = matches!(self.opts.year_to, Some(to) if year > to);
                        if !below && !above {
                            years.push(year);
                        }
                    }
                }
                for year in years.into_iter().rev() {
                    self.stack.push(Frame::Year {
                        model: model.clone(),
                        year,
                    });
                }
            }
            Frame::Year { model, year } => {
                let r = self
                    .fetch(VehiclesOptions {
                        make: Some(make),
                        model: Some(model.clone()),
                        year: Some(year),
                        ..Default::default()
                    })
                    .await?;
                for series in r.data.into_iter().rev() {
                    self.stack.push(Frame::Series {
                        model: model.clone(),
                        year,
                        series,
                    });
                }
            }
            Frame::Series {
                model,
                year,
                series,
            } => {
                let r = self
                    .fetch(VehiclesOptions {
                        make: Some(make),
                        model: Some(model.clone()),
                        year: Some(year),
                        series: Some(series.clone()),
                        ..Default::default()
                    })
                    .await?;
                let engines: Vec<String> = r
                    .data
                    .into_iter()
                    .filter(|e| match &self.needle {
                        Some(n) => e.to_uppercase().contains(n.as_str()),
                        None => true,
                    })
                    .collect();
                for engine in engines.into_iter().rev() {
                    self.stack.push(Frame::Engine {
                        model: model.clone(),
                        year,
                        series: series.clone(),
                        engine,
                    });
                }
            }
            Frame::Engine {
                model,
                year,
                series,
                engine,
            } => {
                let r = self
                    .fetch(VehiclesOptions {
                        make: Some(make),
                        model: Some(model.clone()),
                        year: Some(year),
                        series: Some(series.clone()),
                        engine: Some(engine.clone()),
                        ..Default::default()
                    })
                    .await?;
                for variant in r.data.into_iter().rev() {
                    self.stack.push(Frame::Variant {
                        model: model.clone(),
                        year,
                        series: series.clone(),
                        engine: engine.clone(),
                        variant,
                    });
                }
            }
            Frame::Variant {
                model,
                year,
                series,
                engine,
                variant,
            } => {
                let r = self
                    .fetch(VehiclesOptions {
                        make: Some(make.clone()),
                        model: Some(model.clone()),
                        year: Some(year),
                        series: Some(series.clone()),
                        engine: Some(engine.clone()),
                        variant: Some(variant.clone()),
                        ..Default::default()
                    })
                    .await?;
                for record in r.vehicles {
                    let Some(vehicle_id) = record.vehicle_id else {
                        continue;
                    };
                    let key = WalkKey {
                        vehicle_id,
                        year: if self.opts.per_year { Some(year) } else { None },
                    };
                    if !self.opts.known.insert(key) {
                        continue;
                    }
                    self.output.push_back(WalkedVehicle {
                        vehicle_id,
                        make: make.clone(),
                        model: model.clone(),
                        year,
                        series: series.clone(),
                        engine: engine.clone(),
                        variant: record
                            .select_text
                            .clone()
                            .unwrap_or_else(|| variant.clone()),
                        description: record.description.clone(),
                        long_description: record.long_description.clone(),
                        vehicle_type: self
                            .opts
                            .vehicle_type
                            .clone()
                            .unwrap_or_else(|| "car".to_string()),
                        raw: record.raw,
                    });
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
