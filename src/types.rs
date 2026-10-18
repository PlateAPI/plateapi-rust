use std::collections::HashSet;

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
pub struct Vehicle {
    pub make: Option<String>,
    pub model: Option<String>,
    #[serde(skip)]
    pub year: Option<i32>,
    pub year_range: Option<String>,
    pub lowest_year: Option<i32>,
    pub highest_year: Option<i32>,
    pub body: Option<String>,
    pub engine: Option<String>,
    pub series: Option<String>,
    pub description: Option<String>,
    pub detailed_description: Option<String>,
    /// PlateAPI's stable ID for this exact variant. None when the variant is not certain.
    #[serde(default)]
    pub vehicle_id: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct RateLimit {
    pub limit: Option<i32>,
    pub remaining: Option<i32>,
    pub plan: Option<String>,
    /// Remaining top-up credits, only while they are being consumed.
    pub topup_remaining: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LookupResult {
    pub success: bool,
    pub vehicle: Option<Vehicle>,
    #[serde(default)]
    pub alternatives: Vec<Vehicle>,
    /// "exact", "multiple" or "ambiguous"; None when not found.
    #[serde(rename = "match", default)]
    pub match_: Option<String>,
    pub source: Option<String>,
    pub duration_ms: Option<f64>,
    #[serde(default)]
    pub sandbox: bool,
    pub code: Option<String>,
    pub error: Option<String>,
    pub request_id: Option<String>,
    #[serde(skip)]
    pub rate_limit: RateLimit,
}

impl LookupResult {
    /// Every non-null vehicle_id, the primary vehicle first then the alternatives, without
    /// duplicates. Empty when nothing was found.
    pub fn candidate_ids(&self) -> Vec<i64> {
        let mut ids = Vec::new();
        for v in self.vehicle.iter().chain(self.alternatives.iter()) {
            if let Some(id) = v.vehicle_id {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
        ids
    }
}

/// One record at the vehicle level of the cascade.
#[derive(Debug, Clone)]
pub struct CascadeVehicle {
    pub vehicle_id: Option<i64>,
    pub select_text: Option<String>,
    pub description: Option<String>,
    pub long_description: Option<String>,
    /// The record exactly as returned.
    pub raw: Value,
}

#[derive(Debug, Clone)]
pub struct VehiclesResult {
    pub success: bool,
    pub r#type: Option<String>,
    /// Each value as text: names at most levels, years as digits, and the JSON of each record
    /// at the vehicle level (see `vehicles` for the parsed form).
    pub data: Vec<String>,
    /// Parsed records at the vehicle level; empty at other levels.
    pub vehicles: Vec<CascadeVehicle>,
    pub total: i32,
    pub duration_ms: Option<f64>,
}

impl VehiclesResult {
    /// vehicle_id of each record at the vehicle level; empty at other levels.
    pub fn vehicle_ids(&self) -> Vec<i64> {
        self.vehicles.iter().filter_map(|v| v.vehicle_id).collect()
    }
}

#[derive(Debug, Clone, Default)]
pub struct VehiclesOptions {
    pub make: Option<String>,
    pub model: Option<String>,
    pub year: Option<i32>,
    pub series: Option<String>,
    pub engine: Option<String>,
    pub variant: Option<String>,
    /// "car" (default) or "motorcycle".
    pub vehicle_type: Option<String>,
}

/// One vehicle from GET /api/v1/vehicles/{vehicle_id}.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct VehicleRecord {
    pub vehicle_id: Option<i64>,
    pub vehicle_type: Option<String>,
    pub make: Option<String>,
    pub model: Option<String>,
    pub year_range: Option<String>,
    pub lowest_year: Option<i32>,
    pub highest_year: Option<i32>,
    pub years: Vec<i32>,
    pub description: Option<String>,
    pub long_description: Option<String>,
    pub series: Option<String>,
    pub engine: Option<String>,
    pub variant: Option<String>,
    pub body: Option<String>,
    pub body_size: Option<String>,
    pub doors: Option<i32>,
    pub drive: Option<String>,
    pub transmission: Option<String>,
    /// "full" or "basic".
    pub detail: Option<String>,
    /// The vehicle object exactly as returned.
    #[serde(skip)]
    pub raw: Value,
}

/// Identifies a walked vehicle in `WalkOptions::known`: `year` is None unless `per_year` is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WalkKey {
    pub vehicle_id: i64,
    pub year: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct WalkOptions {
    /// Also walk every model name starting with `model + " "`. Default true.
    pub trims: bool,
    pub year_from: Option<i32>,
    pub year_to: Option<i32>,
    /// Only engines whose name contains this text (case-insensitive).
    pub engine_contains: Option<String>,
    /// "motorcycle" for the motorcycle cascade.
    pub vehicle_type: Option<String>,
    /// One result per vehicle per model year instead of one per vehicle.
    pub per_year: bool,
    /// Vehicles already handled; they are not returned again. The walker keeps it up to date;
    /// read it back with `VehicleWalker::known()` to resume later.
    pub known: HashSet<WalkKey>,
    /// Milliseconds to pause after each request. Default 150.
    pub delay_ms: u64,
}

impl Default for WalkOptions {
    fn default() -> Self {
        Self {
            trims: true,
            year_from: None,
            year_to: None,
            engine_contains: None,
            vehicle_type: None,
            per_year: false,
            known: HashSet::new(),
            delay_ms: 150,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WalkedVehicle {
    pub vehicle_id: i64,
    pub make: String,
    pub model: String,
    pub year: i32,
    pub series: String,
    pub engine: String,
    pub variant: String,
    pub description: Option<String>,
    pub long_description: Option<String>,
    pub vehicle_type: String,
    pub raw: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    pub email: Option<String>,
    pub plan: Option<String>,
    #[serde(default)]
    pub monthly_limit: i32,
    #[serde(default)]
    pub used_this_month: i32,
    #[serde(default)]
    pub remaining: i32,
    pub percent_used: Option<f64>,
    #[serde(default)]
    pub rate_limit_per_min: i32,
    pub last_lookup_at: Option<String>,
    pub period_start: Option<String>,
    pub period_end: Option<String>,
    pub days_remaining: Option<i32>,
    #[serde(default)]
    pub cancel_at_period_end: bool,
    pub cancel_at: Option<String>,
    #[serde(default)]
    pub topup_credits: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LogEntry {
    pub plate: Option<String>,
    pub state: Option<String>,
    #[serde(default)]
    pub success: i32,
    pub error: Option<String>,
    pub duration_ms: Option<f64>,
    pub make: Option<String>,
    pub model: Option<String>,
    pub year: Option<i32>,
    pub client_ip: Option<String>,
    pub request_id: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct LogsOptions {
    pub limit: Option<i32>,
    pub offset: Option<i32>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub plate: Option<String>,
    pub success: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LogsResult {
    #[serde(default)]
    pub logs: Vec<LogEntry>,
    #[serde(default)]
    pub count: i32,
    #[serde(default)]
    pub total: i32,
    #[serde(default)]
    pub limit: i32,
    #[serde(default)]
    pub offset: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HealthStatus {
    pub status: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ClientOptions {
    pub base_url: String,
    pub timeout_secs: u64,
    pub max_retries: u32,
    /// Longest Retry-After (seconds) the SDK will sleep. Default Some(60.0); None waits any length.
    pub max_wait_secs: Option<f64>,
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            base_url: "https://api.plateapi.com.au".to_string(),
            timeout_secs: 30,
            max_retries: 3,
            max_wait_secs: Some(60.0),
        }
    }
}
