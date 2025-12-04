use crate::{CommandResponse, CommandService, Hget, KvError, StatusCode, Value};

impl CommandService for Hget {
    fn execute(self, store: &impl crate::Storage) -> CommandResponse {
        match store.get(&self.table, &self.key) {
            Ok(Some(v)) => v.into(),
            Ok(None) => KvError::NotFound(format!("table {}, key {}", self.table, self.key)).into(),
            Err(e) => e.into(),
        }
    }
}

impl From<Value> for CommandResponse {
    fn from(value: Value) -> Self {
        CommandResponse {
            status: StatusCode::Ok.into(),
            message: "".to_string(),
            values: vec![value],
            pairs: vec![],
        }
    }
}

impl From<KvError> for CommandResponse {
    fn from(e: KvError) -> Self {
        CommandResponse {
            status: StatusCode::InternalServiceError.into(),
            message: e.to_string(),
            values: vec![],
            pairs: vec![],
        }
    }
}
