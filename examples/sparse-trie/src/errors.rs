use alloy_primitives::B256;
use thiserror::Error;

/// Error returned by the state-root task and the parallel proof workers.
#[derive(Error, Debug)]
pub enum StateRootTaskError {
    /// An account whose trie leaf still points at a storage trie was changed or deleted. Storage
    /// updates are out of scope for this task, so it cannot express the resulting state.
    #[error("storage updates are out of scope: {address} holds storage root {storage_root}")]
    StorageOutOfScope {
        /// Hashed address of the account the task refused to overwrite.
        address: B256,
        /// Storage root that the task would have dropped.
        storage_root: B256,
    },
    /// Provider error.
    #[error("provider error")]
    Provider(String),
    /// Proof dispatch error.
    #[error("proof dispatch failed: {_0}")]
    ProofDispatch(String),
    /// A proof worker failed before it could process queued work.
    #[error("proof worker failed: {_0}")]
    ProofWorker(String),
    /// Sparse trie error.
    #[error("sparse trie error: {_0}")]
    SparseTrie(String),
    /// Sparse trie task stalled.
    #[error("sparse trie task stalled")]
    Stalled,
    /// The consumer dropped its cancel guard without waiting for the result.
    #[error("state root task canceled: consumer dropped the handle")]
    Canceled,
    /// recv error
    #[error("transparent")]
    Receive(#[from] crossbeam_channel::RecvError),
    /// Other unspecified error.
    #[error("{_0}")]
    Other(String),
}
