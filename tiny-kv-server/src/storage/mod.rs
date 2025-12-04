use crate::{KvError, Value};

pub trait Storage {
    fn get(&self, table: &str, key: &str) -> Result<Option<Value>, KvError>;
    // set would be multithreading, so interior mutability is enough, that's why &self instead of &mut self is used.
    fn set(&self, table: &str, key: &str, val: Value) -> Result<Option<Value>, KvError>;
}
