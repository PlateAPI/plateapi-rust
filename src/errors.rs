use thiserror::Error;

#[derive(Error, Debug)]
pub enum PlateAPIError {
    #[error("Invalid API key")]
    Authentication,

    #[error("Monthly quota exceeded")]
    QuotaExceeded,

    #[error("Rate limit exceeded")]
    RateLimit {
        retry_after: Option<f64>,
    },

    #[error("Server error ({status})")]
    Server {
        status: u16,
        retry_after: Option<f64>,
    },

    #[error("{message}")]
    Api {
        message: String,
        status: u16,
    },

    #[error("{0}")]
    Request(String),

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
}
