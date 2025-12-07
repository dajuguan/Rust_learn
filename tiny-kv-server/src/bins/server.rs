use std::io;
use tiny_kv_server::{MemStore, ServerService, Service};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    let service = Service::new(MemStore::default());
    loop {
        let (stream, addr) = listener.accept().await.unwrap();
        println!("client:{:?} connected", addr);
        let server = ServerService::new(stream, service.clone());
        tokio::spawn(server.process());
    }
}
