use std::hash::Hash;

use futures::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};

use crate::{
    CommandRequest, CommandResponse, KvError, Service, Value,
    networking::{frame::FrameCoder, stream::StreamAdapter},
};

pub struct ClientService<S, In, Out> {
    inner: StreamAdapter<S, In, Out>,
}

impl<S, In, Out> ClientService<S, In, Out>
where
    S: AsyncRead + AsyncWrite + Unpin,
    In: FrameCoder,
    Out: FrameCoder,
{
    pub fn new(socket: S) -> Self {
        ClientService {
            inner: StreamAdapter::new(socket),
        }
    }

    pub async fn execute(&mut self, cmd: Out) -> Result<In, KvError> {
        let s = &mut self.inner;
        s.send(cmd).await?;
        match s.next().await {
            Some(v) => v,
            None => Err(KvError::Internal("No response".into())),
        }
    }
}

pub struct ServerService<S, In, Out, K: Eq + Hash, V> {
    inner: StreamAdapter<S, In, Out>,
    service: Service<K, V>,
}

impl<S, In, Out, K, V> ServerService<S, In, Out, K, V>
where
    K: Eq + Hash,
    S: AsyncRead + AsyncWrite + Unpin,
    In: FrameCoder,
    Out: FrameCoder,
{
    pub fn new(stream: S, service: Service<K, V>) -> Self {
        Self {
            inner: StreamAdapter::new(stream),
            service,
        }
    }
}

impl<S> ServerService<S, CommandRequest, CommandResponse, String, Value>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    pub async fn process(mut self) -> Result<(), KvError> {
        while let Some(Ok(cmd)) = self.inner.next().await {
            let resp = self.service.execute(cmd);
            self.inner.send(resp).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use prost::bytes::Bytes;
    use tokio::net::{TcpListener, TcpStream};

    use crate::{MemStore, assert_res_ok};

    use super::*;

    // TCPStream is duplex, so dummyStream is not enough for test.
    #[tokio::test]
    async fn client_server_execute_should_work() {
        let addr = start_server().await;
        let socket = TcpStream::connect(addr).await.unwrap();
        let mut client = ClientService::new(socket);

        let v1: Value = "v1".into();
        let cmd = CommandRequest::new_hset("t1", "k1", v1.clone());
        let res: CommandResponse = client.execute(cmd).await.unwrap();
        assert_res_ok(&res, &vec![Value::default()], &[]);

        let cmd = CommandRequest::new_hget("t1", "k1");
        let res: CommandResponse = client.execute(cmd).await.unwrap();
        assert_res_ok(&res, &vec![v1], &[]);
    }

    #[tokio::test]
    async fn client_server_execute_compression_should_work() {
        let addr = start_server().await;
        let socket = TcpStream::connect(addr).await.unwrap();
        let mut client = ClientService::new(socket);

        let v1: Value = Bytes::from(vec![0u8; 100]).into();
        let cmd = CommandRequest::new_hset("t1", "k1", v1.clone());
        let res: CommandResponse = client.execute(cmd).await.unwrap();
        assert_res_ok(&res, &vec![Value::default()], &[]);

        let cmd = CommandRequest::new_hget("t1", "k1");
        let res: CommandResponse = client.execute(cmd).await.unwrap();
        assert_res_ok(&res, &vec![v1], &[]);
    }

    async fn start_server() -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let service = Service::new(MemStore::default());
                let server = ServerService::new(stream, service);
                tokio::spawn(server.process());
            }
        });

        addr
    }
}
