use serde::Deserialize;

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
}

#[derive(Debug, Clone, Default)]
pub struct RateLimit {
    pub limit: Option<i32>,
    pub remaining: Option<i32>,
    pub plan: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LookupResult {
    pub success: bool,
    pub vehicle: Option<Vehicle>,
    #[serde(default)]
    pub alternatives: Vec<Vehicle>,
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

#[derive(Debug, Clone)]
pub struct VehiclesResult {
    pub success: bool,
    pub r#type: Option<String>,
    pub data: Vec<String>,
    pub total: i32,
    pub duration_ms: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct VehiclesOptions {
    pub make: Option<String>,
    pub model: Option<String>,
    pub year: Option<i32>,
    pub series: Option<String>,
    pub engine: Option<String>,
    pub variant: Option<String>,
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
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            base_url: "https://api.plateapi.com.au".to_string(),
            timeout_secs: 30,
            max_retries: 3,
        }
    }
}
