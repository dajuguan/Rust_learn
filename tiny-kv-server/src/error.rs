use thiserror::Error;

#[derive(Error, Debug)]
pub enum KvError {
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("InvalidCommand: {0}")]
    InvalidCommand(String),
}

#[derive(Copy, Clone, Debug)]
pub enum StatusCode {
    Ok = 200,
    InternalServiceError = 500,
}

impl From<StatusCode> for u32 {
    fn from(s: StatusCode) -> Self {
        s as u32
    }
}
