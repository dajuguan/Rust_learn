#[cfg(feature = "kms")]
mod kms_impl;

#[cfg(not(feature = "kms"))]
mod kms_stub;

#[cfg(feature = "kms")]
pub use kms_impl::*;

#[cfg(not(feature = "kms"))]
pub use kms_stub::*;