use crate::{
    CommandResponse, CommandService, Hget, Hset, KvError, MemStore, StatusCode, Storage, Value,
    debug,
};

impl CommandService for Hget {
    fn execute(self, store: &impl Storage<String, Value>) -> CommandResponse {
        match store.get(&self.table, &self.key) {
            Ok(Some(v)) => v.into(),
            Ok(None) => KvError::NotFound(format!("table {}, key {}", self.table, self.key)).into(),
            Err(e) => e.into(),
        }
    }
}

impl CommandService for Hset {
    fn execute(self, store: &impl Storage<String, Value>) -> CommandResponse {
        match self.pair {
            Some(kv) => match store.set(&self.table, kv.key, kv.value.unwrap_or_default()) {
                Ok(Some(v)) => {
                    debug!("prev:{:?}", v);
                    v.into()
                }
                Ok(None) => {
                    debug!("prev none");
                    Value::default().into()
                }
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

#[cfg(test)]
mod tests {
    use crate::{CommandRequest, assert_res_ok, dispatch};

    use super::*;

    #[test]
    fn hset_should_work() {
        let store = MemStore::default();
        let table = "t".to_string();
        let key = "k".to_string();
        let val = 10;
        let req = CommandRequest::new_hset(table.clone(), key.clone(), val.into());
        dispatch(req, &store);

        let req = CommandRequest::new_hset(table, key, val.into());
        let res = dispatch(req, &store);
        assert_res_ok(&res, &vec![val.into()], &vec![]);
    }

    #[test]
    fn hget_should_work() {
        let store = MemStore::default();
        let table = "t".to_string();
        let key = "k".to_string();
        let val = 10;
        let req = CommandRequest::new_hset(table.clone(), key.clone(), val.into());
        dispatch(req, &store);

        let req = CommandRequest::new_hget(table, key);
        let res = dispatch(req, &store);
        assert_res_ok(&res, &vec![val.into()], &vec![]);
    }
}
