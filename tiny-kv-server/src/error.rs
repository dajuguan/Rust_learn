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
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("Fail to decrypt or encrypt")]
    AheadCipherError(#[from] chacha20poly1305::Error),
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

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {{
        #[cfg(debug_assertions)]
        {
            eprintln!(
                "[DEBUG][{}:{}] {}",
                file!(),
                line!(),
                format!($($arg)*)
            );
        }
    }};
}
