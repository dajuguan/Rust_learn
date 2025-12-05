use crate::{CommandResponse, CommandService, Hget, Hset, KvError, StatusCode, Value};

impl CommandService for Hget {
    fn execute(self, store: &impl crate::Storage<String, Value>) -> CommandResponse {
        match store.get(&self.table, &self.key) {
            Ok(Some(v)) => v.into(),
            Ok(None) => KvError::NotFound(format!("table {}, key {}", self.table, self.key)).into(),
            Err(e) => e.into(),
        }
    }
}

impl CommandService for Hset {
    fn execute(self, store: &impl crate::Storage<String, Value>) -> CommandResponse {
        match self.pair {
            Some(kv) => match store.set(&self.table, kv.key, kv.value.unwrap_or_default()) {
                Ok(Some(v)) => v.into(),
                Ok(None) => Value::default().into(),
                Err(e) => e.into(),
            },
            None => KvError::InvalidCommand(format!("{:?}", self)).into(),
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
