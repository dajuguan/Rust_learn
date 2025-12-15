use std::{
    collections::HashMap,
    sync::Arc,
    task::{Context, ready},
};

use crate::KvError;

use dashmap::DashMap;
use futures::FutureExt;
use prost::bytes::{BufMut, BytesMut};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadHalf},
    sync::{
        Mutex,
        mpsc::{self, Receiver, Sender},
    },
};

/* frame:
len | frame id | palyload
 */

const STREAM_ID_LEN: usize = 4;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StreamId(u32);

#[derive(Debug, Clone, Copy)]
pub enum ConnMode {
    Server,
    Client,
}

pub struct YamuxConnection<S, In, Out> {
    reader: ReadHalf<S>,
    next_id: StreamId,
    mode: ConnMode,
    tx_to_logic: HashMap<StreamId, Sender<Out>>,
    tx_to_conn: Sender<In>,
    streams: DashMap<StreamId, Arc<LogicStream>>,
}

impl<S> YamuxConnection<S, LogicFrame, LogicFrame>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    pub fn new(stream: S, mode: ConnMode) -> Self
// In: LogicFrameCoder + Send + 'static,
// Out: LogicFrameCoder + Send + 'static,
    {
        let (tx, mut rx) = mpsc::channel::<LogicFrame>(100);

        let (reader, mut writter) = tokio::io::split(stream);
        tokio::spawn(async move {
            while let Some(frame) = rx.recv().await {
                println!(
                    "mode:{}, write frame id:{}, payload:{:?}",
                    match mode {
                        ConnMode::Client => "Client",
                        ConnMode::Server => "Server",
                    },
                    frame.id.0,
                    String::from_utf8_lossy(&frame.payload)
                );
                let mut buf = BytesMut::new();
                buf.put_u32(frame.len() as _);
                buf.put_u32(frame.id.0);
                buf.unsplit(frame.payload);
                let _ = writter.write_all(&buf[..]).await;
            }
        });

        tokio::spawn(async {});

        let conn = Self {
            reader,
            mode,
            next_id: match mode {
                ConnMode::Client => StreamId(1),
                ConnMode::Server => StreamId(2),
            },
            tx_to_logic: HashMap::new(),
            tx_to_conn: tx,
            streams: DashMap::new(),
        };

        conn
    }

    pub async fn open_stream(&mut self) -> Result<LogicStream, KvError> {
        let (tx, mut rx) = mpsc::channel::<LogicFrame>(100);
        let stream_id = self.next_id;
        self.next_id = StreamId(self.next_id.0 + 2);
        self.tx_to_logic.insert(stream_id, tx);

        Ok(LogicStream {
            id: stream_id,
            rx_from_conn: Mutex::new(rx),
            tx_to_conn: self.tx_to_conn.clone(),
        })
    }

    async fn read_loop(&mut self) {
        // todo: handle close
        while self.tx_to_logic.len() > 0 {
            // read frame from underlying connection
            // parse frame id
            // dispatch to corresponding logic stream
            let payload_len = self.reader.read_u32().await.unwrap();
            let stream_id = self.reader.read_u32().await.unwrap();
            let payload_len = payload_len as usize;
            let mut payload = BytesMut::with_capacity(payload_len);
            unsafe { payload.advance_mut(payload_len) };
            self.reader.read_exact(&mut payload).await.unwrap();
            let frame: LogicFrame = LogicFrame {
                id: StreamId(stream_id),
                payload,
            };
            if let Some(tx) = self.tx_to_logic.get(&StreamId(stream_id)) {
                let _ = tx.send(frame).await;
            } else {
                // unknown stream id, ignore
                panic!(
                    "unknown stream id: {:?},  len: {}, payload: {:?}",
                    stream_id,
                    payload_len,
                    String::from_utf8_lossy(&frame.payload)
                );
            }
        }
    }

    pub fn poll_next_inbound(
        &mut self,
        cx: &mut Context<'_>,
    ) -> std::task::Poll<Option<Result<Arc<LogicStream>, KvError>>> {
        let fut = self.reader.read_u32();
        let payload_len = ready!(Box::pin(fut).poll_unpin(cx)).unwrap();
        let fut = self.reader.read_u32();
        let stream_id = ready!(Box::pin(fut).poll_unpin(cx)).unwrap();
        let mut payload = BytesMut::with_capacity(payload_len as _);
        unsafe { payload.advance_mut(payload_len as _) };
        let fut = self.reader.read_exact(&mut payload[..]);
        ready!(Box::pin(fut).poll_unpin(cx)).unwrap();

        println!("buf len: {}, {}", payload.len(), payload_len);
        let frame = LogicFrame {
            id: StreamId(stream_id),
            payload,
        };

        let stream_id = StreamId(stream_id);

        if self.streams.contains_key(&stream_id) {
            let tx = self.tx_to_logic.get(&stream_id).unwrap();
            // send frame to logic stream
            let fut = tx.send(frame);
            ready!(Box::pin(fut).poll_unpin(cx)).unwrap();
            std::task::Poll::Ready(None)
        } else {
            let new_stream = LogicStream {
                id: stream_id,
                rx_from_conn: Mutex::new({
                    let (tx, rx) = mpsc::channel::<LogicFrame>(100);
                    let fut = tx.send(frame);
                    ready!(Box::pin(fut).poll_unpin(cx)).unwrap();
                    self.tx_to_logic.insert(stream_id, tx);
                    rx
                }),
                tx_to_conn: self.tx_to_conn.clone(),
            };

            std::task::Poll::Ready(Some(Ok(Arc::new(new_stream))))
        }
    }
}

pub trait LogicFrameCoder
where
    Self: Sized,
{
    fn decode_frame(buf: &mut BytesMut) -> Result<Self, KvError> {
        todo!()
    }

    fn encode_frame(&self, buf: &mut BytesMut) -> Result<(), KvError> {
        todo!()
    }
}

pub struct LogicFrame {
    id: StreamId,
    payload: BytesMut,
}

impl LogicFrame {
    pub fn len(&self) -> usize {
        self.payload.len()
    }
}

pub struct LogicStream {
    // fields for logic stream
    id: StreamId, // stream identifier
    rx_from_conn: Mutex<Receiver<LogicFrame>>,
    tx_to_conn: Sender<LogicFrame>,
}

impl LogicStream {
    async fn read(&self) -> Result<Option<Vec<u8>>, KvError> {
        let mut rx = self.rx_from_conn.lock().await;
        rx.recv()
            .await
            .map(|frame| Some(frame.payload.to_vec()))
            .ok_or_else(|| KvError::Internal("Connection closed".into()))
    }
    async fn write(&self, data: &[u8]) -> Result<(), KvError> {
        let frame = LogicFrame {
            id: self.id,
            payload: BytesMut::from(data),
        };
        self.tx_to_conn
            .send(frame)
            .await
            .map_err(|_| KvError::Internal("Failed to send frame to connection".into()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{TryStreamExt, stream};
    use tokio::net::TcpListener;
    use tokio::net::TcpStream;

    pub async fn echo_server<T>(
        mut c: YamuxConnection<T, LogicFrame, LogicFrame>,
    ) -> Result<(), KvError>
    where
        T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        stream::poll_fn(|cx| c.poll_next_inbound(cx))
            .try_for_each_concurrent(None, |stream| async move {
                while let Ok(Some(payload)) = stream.read().await {
                    println!(
                        "server received on stream {}: {:?}",
                        stream.id.0,
                        String::from_utf8_lossy(&payload)
                    );
                    let addon = b"resp to ".to_vec();
                    let data = [addon, payload].concat();
                    stream.write(&data).await.unwrap();
                }
                Ok(())
            })
            .await
    }

    #[tokio::test]
    async fn test_yamux_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let conn = YamuxConnection::new(stream, ConnMode::Server);
            echo_server(conn).await.unwrap();
        });

        let stream = TcpStream::connect(addr).await.unwrap();
        let mut conn = YamuxConnection::new(stream, ConnMode::Client);

        let s1 = conn.open_stream().await.unwrap();
        let s2 = conn.open_stream().await.unwrap();

        let h1 = tokio::spawn(async move {
            conn.read_loop().await;
        });

        let h2 = tokio::spawn(async move {
            s1.write("hello from s1".as_bytes()).await.unwrap();
            let msg1 = s1.read().await.unwrap().unwrap();
            println!("s1 msg1: {}", String::from_utf8_lossy(&msg1));

            s1.write("hello1 from s1".as_bytes()).await.unwrap();
            let msg1 = s1.read().await.unwrap().unwrap();
            println!("s1 msg2: {}", String::from_utf8_lossy(&msg1));

            s1.write("close from s1".as_bytes()).await.unwrap();
        });

        let h3 = tokio::spawn(async move {
            s2.write("hello from s2".as_bytes()).await.unwrap();
            let msg1 = s2.read().await.unwrap().unwrap();
            println!("s2 msg1: {}", String::from_utf8_lossy(&msg1));
        });

        let _ = tokio::join!(h1, h2, h3);
    }
}
