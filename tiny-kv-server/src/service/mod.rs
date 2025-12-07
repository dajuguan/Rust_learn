mod cmd_service;

use std::{hash::Hash, marker::PhantomData, sync::Arc};

use crate::{
    CommandRequest, CommandResponse, MemStore, Storage, Value, command_request::RequestData,
};

pub trait CommandService {
    fn execute(self, store: &impl Storage<String, Value>) -> CommandResponse;
}

pub fn dispatch(cmd_req: CommandRequest, store: &impl Storage<String, Value>) -> CommandResponse {
    match cmd_req.request_data.unwrap() {
        RequestData::Hget(params) => params.execute(store),
        RequestData::Hset(params) => params.execute(store),
        _ => CommandResponse::default(),
    }
}

pub struct Service<K: Eq + Hash, V, S = MemStore<K, V>> {
    inner: Arc<ServiceInner<S>>,
    _k: PhantomData<K>,
    _v: PhantomData<V>,
}

impl<K, V, S> Service<K, V, S>
where
    K: Eq + Hash,
    S: Storage<K, V>,
{
    pub fn new(store: S) -> Self {
        let inner = ServiceInner { store };
        Self {
            inner: Arc::new(inner),
            _k: PhantomData::default(),
            _v: PhantomData::default(),
        }
    }
}

impl<K, V, S> Clone for Service<K, V, S>
where
    K: Eq + Hash,
    S: Storage<K, V>,
{
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            _k: self._k,
            _v: self._v,
        }
    }
}

impl<K, V, S> Service<K, V, S>
where
    K: Eq + Hash,
    S: Storage<String, Value>,
{
    pub fn execute(&self, cmd: CommandRequest) -> CommandResponse {
        dispatch(cmd, &self.inner.store)
    }
}

struct ServiceInner<S> {
    store: S,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn service_should_work() {
        let service = Service::new(MemStore::default());
        let s_clone = service.clone();

        let cmd = CommandRequest::new_hset("t1", "k1", "v1".into());
        tokio::spawn(async move {
            let res = s_clone.execute(cmd);
            assert_res_ok(&res, &[Value::default()], &[]);
        })
        .await
        .unwrap();
    }
}

#[cfg(test)]
pub fn assert_res_ok(res: &CommandResponse, values: &[Value], pairs: &[crate::Kvpair]) {
    use crate::StatusCode;

    let mut sorted_pairs = res.pairs.clone();
    sorted_pairs.sort_by(|a, b| a.partial_cmp(b).unwrap());

    assert_eq!(res.status, StatusCode::Ok.into());
    assert_eq!(res.message, "");
    assert_eq!(res.values, values);
    assert_eq!(sorted_pairs, pairs);
}
