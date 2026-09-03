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
