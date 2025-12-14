mod cmd_service;
mod notification;

use std::{hash::Hash, marker::PhantomData, sync::Arc};

use crate::{
    CommandRequest, CommandResponse, MemStore, Storage, Value,
    command_request::RequestData,
    service::{notification::Notify, notification::NotifyMut},
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

impl<K: Eq + Hash, V, S> From<ServiceInner<S>> for Service<K, V, S>
where
    S: Storage<K, V>,
{
    fn from(inner: ServiceInner<S>) -> Self {
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
        self.inner.on_req.notify(&cmd);
        let mut resp = dispatch(cmd, &self.inner.store);
        self.inner.on_exe.notify(&resp);
        self.inner.on_before_send.notify(&mut resp);
        resp
    }
}

pub struct ServiceInner<S> {
    store: S,
    on_req: Vec<fn(&CommandRequest)>,
    on_exe: Vec<fn(&CommandResponse)>,
    on_before_send: Vec<fn(&mut CommandResponse)>,
}

impl<S> ServiceInner<S> {
    pub fn new(store: S) -> Self {
        Self {
            store,
            on_req: vec![],
            on_exe: vec![],
            on_before_send: vec![],
        }
    }

    pub fn fn_on_req(mut self, f: fn(&CommandRequest)) -> Self {
        self.on_req.push(f);
        self
    }

    pub fn fn_on_exe(mut self, f: fn(&CommandResponse)) -> Self {
        self.on_exe.push(f);
        self
    }

    pub fn fn_on_before_send(mut self, f: fn(&mut CommandResponse)) -> Self {
        self.on_before_send.push(f);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn service_should_work() {
        let service: Service<String, Value, _> = ServiceInner::new(MemStore::default()).into();
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
    assert_eq!(res.values, values);
    assert_eq!(sorted_pairs, pairs);
}
