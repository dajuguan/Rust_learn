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

/// Database error type.
#[derive(Clone, Debug, thiserror::Error)]
pub enum DatabaseError {
    /// Failed to open the database.
    #[error("failed to open the database: {_0}")]
    Open(String),
    /// Failed to create a table in the database.
    #[error("failed to create a table: {_0}")]
    CreateTable(String),
    /// Failed to write a value into a table.
    #[error("failed to write a value into a database table: {_0}")]
    Write(String),
    /// Failed to read a value from a table.
    #[error("failed to read a value from a database table: {_0}")]
    Read(String),
    /// Failed to delete a `(key, value)` pair from a table.
    #[error("database delete error code: {_0}")]
    Delete(String),
    /// Failed to commit transaction changes into the database.
    #[error("failed to commit transaction changes: {_0}")]
    Commit(String),
}
