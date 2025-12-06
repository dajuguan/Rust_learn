use std::error::Error;

use std::{io, vec};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;

    loop {
        let (mut socket, addr) = listener.accept().await?;
        println!("new client: {:?}", addr);
        let mut buf = vec![];
        socket.read_to_end(&mut buf).await;
        println!("got :{:?}", String::from_utf8_lossy(&buf[..]));
        process_socket(socket).await;
    }
}

async fn process_socket<T>(socket: T) {
    // do work with socket here
}
