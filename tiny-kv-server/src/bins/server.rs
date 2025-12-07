use prost::bytes::{BufMut, BytesMut};
use std::{io, vec};
use tiny_kv_server::{CommandResponse, FrameCoder};
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;

    loop {
        let (mut socket, addr) = listener.accept().await?;
        println!("new client: {:?}", addr);
        let mut buf = vec![];
        socket.read_to_end(&mut buf).await.unwrap();
        let mut data = BytesMut::new();
        data.put(&buf[..]);
        let res = CommandResponse::decode_frame(&mut data);
        println!("got :{:?}", res);
        process_socket(socket).await;
    }
}

async fn process_socket<T>(socket: T) {
    // do work with socket here
}

// s.process()
// while let Some(req) = s.next().await?
//  - resp = dispatch(req, store)
//  - s.send(resp).await
