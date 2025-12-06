use thiserror::Error;

#[derive(Error, Debug)]
pub enum KvError {
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("InvalidCommand: {0}")]
    InvalidCommand(String),
    #[error("Frame size exceeds max size")]
    FrameSizeError,
    #[error("Failed to encode protobuf message")]
    EncodeError(#[from] prost::EncodeError),
    #[error("Failed to decode protobuf message")]
    DecodeError(#[from] prost::DecodeError),
    #[error("Failed to read buf caused by I/O error ")]
    IoError(#[from] std::io::Error),
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
