use std::io;
use tiny_kv_server::{MemStore, SecureStream, ServerService, Service, ServiceInner, Value};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    let inner = ServiceInner::new(MemStore::default())
        .fn_on_req(|cmd| {
            println!("server get req:{:?}", cmd);
        })
        .fn_on_exe(|resp| {
            println!("server get resp:{:?}", resp);
        })
        .fn_on_before_send(|resp| {
            resp.message = "modified before send".to_string();
            println!("server change resp before send:{:?}", resp);
        });

    let service: Service<String, Value, _> = inner.into();
    loop {
        let (stream, addr) = listener.accept().await.unwrap();
        println!("client:{:?} connected", addr);
        let secure_s = SecureStream::new(stream);
        let server = ServerService::new(secure_s, service.clone());
        tokio::spawn(server.process());
    }
}
