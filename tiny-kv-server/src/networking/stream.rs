use std::{
    marker::PhantomData,
    task::{Poll, ready},
};

use futures::{FutureExt, Sink, Stream};
use prost::bytes::BytesMut;
use std::pin::Pin;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

use crate::{
    KvError,
    networking::frame::{FrameCoder, read_frame},
};

pub struct StreamAdapter<S, In, Out> {
    stream: S,
    wbuf: BytesMut, // write buf to cache unfinished write
    wlen: usize,
    rbuf: BytesMut,         // read buf to cache unfinished read
    _in: PhantomData<In>,   // for Stream's associate Item
    _out: PhantomData<Out>, // for Sink's associate Item
}

// unpin will implement deref for StreamAdapter, which means Pin is almost not existing.
impl<S, In, Out> Unpin for StreamAdapter<S, In, Out> where S: Unpin {}

impl<S, In, Out> StreamAdapter<S, In, Out> {
    pub fn new(s: S) -> Self
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        Self {
            stream: s,
            wbuf: BytesMut::new(),
            wlen: 0,
            rbuf: BytesMut::new(),
            _in: PhantomData::default(),
            _out: PhantomData::default(),
        }
    }
}

impl<S, In, Out> Stream for StreamAdapter<S, In, Out>
where
    S: AsyncRead + AsyncWrite + Unpin,
    In: FrameCoder,
{
    type Item = Result<In, KvError>;
    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        assert!(self.rbuf.is_empty());

        // reset the visible part to reuse the same rbuf instead of allocating one each time
        let mut rest = self.rbuf.split_off(0);
        let fut = read_frame(&mut self.stream, &mut rest);
        ready!(Box::pin(fut).poll_unpin(cx))?;
        self.rbuf.unsplit(rest);
        Poll::Ready(Some(In::decode_frame(&mut self.rbuf)))
    }
}

impl<S, In, Out> Sink<Out> for StreamAdapter<S, In, Out>
where
    S: AsyncRead + AsyncWrite + Unpin,
    In: FrameCoder,
    Out: FrameCoder,
{
    type Error = KvError;
    fn poll_ready(
        self: Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: Out) -> Result<(), Self::Error> {
        item.encoded_frame(&mut self.get_mut().wbuf)?;
        Ok(())
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        let this = self.get_mut();

        while this.wlen != this.wbuf.len() {
            let fut = Pin::new(&mut this.stream).poll_write(cx, &this.wbuf[this.wlen..]);
            let written = ready!(fut)?;
            this.wlen += written;
        }

        // clear wbuf
        this.wbuf.clear();
        this.wlen = 0;

        ready!(Pin::new(&mut this.stream).poll_flush(cx)?);
        Poll::Ready(Ok(()))
    }

    fn poll_close(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        ready!(self.as_mut().poll_flush(cx))?;

        // close stream
        ready!(Pin::new(&mut self.stream).poll_shutdown(cx))?;
        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use futures::{SinkExt, StreamExt};

    use crate::{CommandRequest, utils::DummyStream};

    use super::*;

    #[tokio::test]
    async fn test_steam_adaptor_should_work() {
        let buf = BytesMut::new();
        let s = DummyStream { buf };
        let mut s = StreamAdapter::new(s);

        let cmd = CommandRequest::new_hget("t1", "k1");
        s.send(cmd.clone()).await.unwrap();

        if let Some(Ok(s)) = s.next().await {
            assert_eq!(cmd, s);
        } else {
            assert!(false);
        }
    }
}
