use std::error::Error;

use futures::future::join_all;
use tiny_kv_server::{ClientService, CommandRequest, CommandResponse, SecureStream};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let n = 10; // num clients
    let mut tasks = Vec::with_capacity(n);
    for i in 0..n {
        tasks.push(tokio::spawn(async move {
            let stream = TcpStream::connect("127.0.0.1:8080").await.unwrap();
            let secure_s = SecureStream::new(stream);

            let mut client = ClientService::new(secure_s);

            let cmd = CommandRequest::new_hset("t1", "k1", i.to_string().into());
            let res: CommandResponse = client.execute(cmd).await.unwrap();
            println!("Client:{}, got set resp: {:?}", i, res);

            let cmd = CommandRequest::new_hget("t1", "k1");
            let res: CommandResponse = client.execute(cmd).await.unwrap();
            println!("Client:{}, got get resp: {:?}", i, res);
        }));
    }

    join_all(tasks).await;

    Ok(())
}
