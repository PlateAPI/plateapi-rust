use thiserror::Error;

#[derive(Error, Debug)]
pub enum PlateAPIError {
    /// HTTP 401: missing, invalid or deactivated API key, or email not verified.
    #[error("Invalid API key")]
    Authentication,

    /// HTTP 429 because the monthly lookup quota is used up (Retry-After is 3600 s).
    #[error("Monthly quota exceeded")]
    QuotaExceeded,

    /// HTTP 429 per-minute limit. retry_after is the server's Retry-After in seconds, if sent.
    #[error("Rate limit exceeded")]
    RateLimit { retry_after: Option<f64> },

    /// HTTP 5xx after retries.
    #[error("Server error ({status})")]
    Server {
        status: u16,
        retry_after: Option<f64>,
    },

    /// Any other 4xx. The message is the API's own.
    #[error("{message}")]
    Api { message: String, status: u16 },

    /// Connection failure, timeout or unreadable response.
    #[error("{0}")]
    Request(String),

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    /// HTTP 400: invalid plate, state, cascade value or vehicle ID.
    #[error("{message}")]
    BadRequest {
        message: String,
        request_id: Option<String>,
    },

    /// HTTP 403: IP not on the key's allowlist, account banned, or the vehicle database needs a
    /// Starter plan or above.
    #[error("{message}")]
    PermissionDenied {
        message: String,
        request_id: Option<String>,
    },

    /// HTTP 404: no vehicle with that vehicle_id.
    #[error("{message}")]
    NotFound {
        message: String,
        code: Option<String>,
        request_id: Option<String>,
    },
}

impl PlateAPIError {
    /// The HTTP status behind this error, when there was a response.
    pub fn status_code(&self) -> Option<u16> {
        match self {
            PlateAPIError::Authentication => Some(401),
            PlateAPIError::QuotaExceeded | PlateAPIError::RateLimit { .. } => Some(429),
            PlateAPIError::Server { status, .. } | PlateAPIError::Api { status, .. } => {
                Some(*status)
            }
            PlateAPIError::BadRequest { .. } => Some(400),
            PlateAPIError::PermissionDenied { .. } => Some(403),
            PlateAPIError::NotFound { .. } => Some(404),
            PlateAPIError::Request(_) | PlateAPIError::InvalidArgument(_) => None,
        }
    }

    /// The server's Retry-After in seconds, when it sent one (3600 for QuotaExceeded).
    pub fn retry_after(&self) -> Option<f64> {
        match self {
            PlateAPIError::QuotaExceeded => Some(3600.0),
            PlateAPIError::RateLimit { retry_after }
            | PlateAPIError::Server { retry_after, .. } => *retry_after,
            _ => None,
        }
    }

    /// The API's machine code, when known (not_found, quota_exceeded).
    pub fn code(&self) -> Option<&str> {
        match self {
            PlateAPIError::QuotaExceeded => Some("quota_exceeded"),
            PlateAPIError::NotFound { code, .. } => code.as_deref(),
            _ => None,
        }
    }

    /// The request ID, when the response carried one. Quote it when contacting support.
    pub fn request_id(&self) -> Option<&str> {
        match self {
            PlateAPIError::BadRequest { request_id, .. }
            | PlateAPIError::PermissionDenied { request_id, .. }
            | PlateAPIError::NotFound { request_id, .. } => request_id.as_deref(),
            _ => None,
        }
    }

    /// True for both kinds of 429 (RateLimit and QuotaExceeded).
    pub fn is_rate_limited(&self) -> bool {
        matches!(
            self,
            PlateAPIError::RateLimit { .. } | PlateAPIError::QuotaExceeded
        )
    }
}
