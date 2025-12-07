use std::error::Error;

use futures::SinkExt;
use tiny_kv_server::{CommandRequest, CommandResponse, StreamAdapter};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Connect to a peer
    let stream = TcpStream::connect("127.0.0.1:8080").await?;

    let mut stream: StreamAdapter<TcpStream, CommandResponse, CommandRequest> =
        StreamAdapter::new(stream);

    let cmd = CommandRequest::new_hset("t1", "k1", "v1".into());
    stream.send(cmd).await?;

    Ok(())
}
