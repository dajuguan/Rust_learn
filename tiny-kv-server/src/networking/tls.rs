use std::{
    io::Error,
    pin::Pin,
    task::{Poll, ready},
};

use chacha20poly1305::{
    ChaCha20Poly1305, Key, Nonce,
    aead::{Aead, KeyInit},
};
use prost::bytes::{BufMut, BytesMut};
use rand::prelude::*;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::{KvError, debug};

/*A simple symetric encrypted stream without tls handshake.
stream => secureStream => streamAdaptor
write: payload => StreamAdaptor.writeStream(encodeFrame) => secureStream.write(encode) => stream.write
read: stream.read => secureStream.read(decodeFrame) => StreamAdaptor.read(decode) =>  => payload

data frame:
HEADER_LEN | encrypted payload
 */

const HEADER_LEN: usize = 4;
// Cipher trait
pub trait AeadCipher: Send + Sync + 'static {
    const NONCE_LEN: usize;
    fn encrypt(&self, nonce: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, KvError>;
    fn decrypt(&self, nonce: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, KvError>;
}

pub struct SecureStream<S, C = ChaCha20Poly1305Cipher> {
    stream: S,
    cipher: C,
    // internal read state for poll_read to cache buffer incase poll_read's buf len is not enough.
    read_state: ReadState,
}

enum ReadState {
    ReadHeader {
        buf: [u8; HEADER_LEN],
        pos: usize,
    },
    ReadEncrypted {
        total: usize,
        buf: BytesMut,
        pos: usize,
    },
    OutputPlain {
        data: BytesMut,
        pos: usize,
    },
}

impl<S> SecureStream<S, ChaCha20Poly1305Cipher>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    pub fn new(stream: S) -> Self {
        Self {
            stream,
            cipher: ChaCha20Poly1305Cipher::default(),
            read_state: ReadState::ReadHeader {
                buf: [0; HEADER_LEN],
                pos: 0,
            },
        }
    }
}

impl<S, C> AsyncWrite for SecureStream<S, C>
where
    S: AsyncRead + AsyncWrite + Unpin,
    C: AeadCipher + Unpin,
{
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();

        let mut nonce = vec![0u8; C::NONCE_LEN];
        rand::rng().fill_bytes(&mut nonce);
        // 1.get encode payload
        let ciphertext = match this.cipher.encrypt(&nonce[..], buf) {
            Ok(c) => c,
            Err(e) => return Poll::Ready(Err(Error::new(std::io::ErrorKind::InvalidData, e))),
        };
        // 2.write len and encrypted payload
        let mut frame = BytesMut::new();
        let total_len = nonce.len() + ciphertext.len();
        debug!(
            "tls buf len:{}, cipher len:{}, total_len:{}",
            buf.len(),
            ciphertext.len(),
            total_len
        );
        frame.put_u32(total_len as _);
        frame.put_slice(&nonce);
        frame.put_slice(&ciphertext);
        ready!(Pin::new(&mut this.stream).poll_write(cx, &frame)?);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().stream).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        ready!(self.as_mut().poll_flush(cx))?;
        Pin::new(&mut self.get_mut().stream).poll_shutdown(cx)
    }
}

// Wrong way to do it! because buf's length can't be resize in this function, when plain_frame > buf.len, it'll panic.
// So, we'll have to maintain a state machine to write buf.

// impl<S, C> AsyncRead for SecureStream<S, C>
// where
//     S: AsyncRead + AsyncWrite + Unpin,
//     C: AeadCipher + Unpin,
// {
//     fn poll_read(
//         self: Pin<&mut Self>,
//         cx: &mut std::task::Context<'_>,
//         buf: &mut tokio::io::ReadBuf<'_>,
//     ) -> Poll<std::io::Result<()>> {
//         let this = self.get_mut();
//         // 1. read payload len
//         let mut len_bytes = [0u8; HEADER_LEN];
//         ready!(Pin::new(&mut this.stream).poll_read(cx, &mut ReadBuf::new(&mut len_bytes)))?;
//         let total_len: u32 = u32::from_be_bytes(len_bytes);

//         // 2. read nonce + payload
//         let mut encrypted = vec![0; total_len as _];
//         ready!(Pin::new(&mut this.stream).poll_read(cx, &mut ReadBuf::new(&mut encrypted)))?;

//         let mut f = BytesMut::new();
//         f.put_slice(&encrypted);
//         let (nonce, ciphertext) = encrypted.split_at(C::NONCE_LEN);
//         let plain_frame = match this.cipher.decrypt(nonce, ciphertext) {
//             Ok(p) => p,
//             Err(e) => return Poll::Ready(Err(Error::new(std::io::ErrorKind::InvalidData, e))),
//         };
//         // plain_frame = header + payload  in stream.rs
//         // buf.initialize_unfilled_to(plain_frame.len());
//         buf.put_slice(&plain_frame);
//         Poll::Ready(Ok(()))
//     }
// }

impl<S, C> AsyncRead for SecureStream<S, C>
where
    S: AsyncRead + AsyncWrite + Unpin,
    C: AeadCipher + Unpin,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        out: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        loop {
            match &mut this.read_state {
                // -------------------------------------------------------------
                // 1. read 4 bytes first for length of nonce+ciphertext
                // -------------------------------------------------------------
                ReadState::ReadHeader { buf, pos } => {
                    while *pos < HEADER_LEN {
                        let mut tmp = ReadBuf::new(&mut buf[*pos..]);
                        let n = match Pin::new(&mut this.stream).poll_read(cx, &mut tmp)? {
                            Poll::Ready(()) => tmp.filled().len(),
                            Poll::Pending => return Poll::Pending,
                        };
                        *pos += n;
                        if n == 0 {
                            return Poll::Ready(Err(Error::from(
                                std::io::ErrorKind::UnexpectedEof,
                            )));
                        }
                    }

                    let total = u32::from_be_bytes(*buf) as usize;
                    this.read_state = ReadState::ReadEncrypted {
                        total,
                        buf: BytesMut::with_capacity(total),
                        pos: 0,
                    };
                }

                // -------------------------------------------------------------
                // 2. read encrypted(nonce + ciphertext)
                // -------------------------------------------------------------
                ReadState::ReadEncrypted { total, buf, pos } => {
                    while *pos < *total {
                        let mut tail = buf.split_off(*pos);
                        // shouldn't use ReadBuf::new(&mut tail), because the deref operator only outputs tail[..len==0],
                        // thus tmp's length will be 0, which causes tmp couldn't read any data.
                        let uninit_slice = unsafe { tail.chunk_mut().as_uninit_slice_mut() };
                        let mut tmp = ReadBuf::uninit(uninit_slice);

                        let n = match Pin::new(&mut this.stream).poll_read(cx, &mut tmp)? {
                            Poll::Ready(()) => tmp.filled().len(),
                            Poll::Pending => return Poll::Pending,
                        };

                        if n == 0 {
                            return Poll::Ready(Err(Error::from(
                                std::io::ErrorKind::UnexpectedEof,
                            )));
                        }

                        // must merge, because split_off has changed buf to buf[..*pos]
                        buf.unsplit(tail);
                        unsafe { buf.advance_mut(n) };
                        *pos += n;
                    }

                    // split nonce + ciphertext
                    let encrypted = buf.split().freeze();
                    let (nonce, ciphertext) = encrypted.split_at(C::NONCE_LEN);

                    // decrypt
                    let plain = this
                        .cipher
                        .decrypt(nonce, ciphertext)
                        .map_err(|e| Error::new(std::io::ErrorKind::InvalidData, e))?;

                    this.read_state = ReadState::OutputPlain {
                        data: BytesMut::from(&plain[..]),
                        pos: 0,
                    };
                }

                // -------------------------------------------------------------
                // 3. output plain frame
                // -------------------------------------------------------------
                ReadState::OutputPlain { data, pos } => {
                    let remaining = data.len() - *pos;
                    let can_copy = remaining.min(out.remaining());

                    if can_copy == 0 {
                        // out buf is full，wait for next poll_read
                        return Poll::Pending;
                    }

                    let out_slice = &data[*pos..(*pos + can_copy)];
                    out.put_slice(out_slice);
                    *pos += can_copy;

                    if *pos == data.len() {
                        // all frame data has been sent, reset inner state to ReadHeader.
                        this.read_state = ReadState::ReadHeader {
                            buf: [0; HEADER_LEN],
                            pos: 0,
                        };
                    }

                    return Poll::Ready(Ok(()));
                }
            }
        }
    }
}

#[derive(Default)]
pub struct ChaCha20Poly1305Cipher {
    key: Key,
}

impl AeadCipher for ChaCha20Poly1305Cipher {
    const NONCE_LEN: usize = 12;

    fn encrypt(&self, nonce: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, KvError> {
        let cipher = ChaCha20Poly1305::new(&self.key);
        Ok(cipher.encrypt(Nonce::from_slice(nonce), plaintext)?)
    }

    fn decrypt(&self, nonce: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, KvError> {
        let cipher = ChaCha20Poly1305::new(&self.key);
        Ok(cipher.decrypt(Nonce::from_slice(nonce), ciphertext)?)
    }
}

#[cfg(test)]
mod tests {
    use futures::{SinkExt, StreamExt};

    use crate::{COMPRESSION_LIMIT, CommandRequest, StreamAdapter, utils::DummyStream};

    use super::*;

    #[tokio::test]
    async fn test_secure_stream_should_work() {
        let buf = BytesMut::new();
        let s = DummyStream { buf };
        let secure_s: SecureStream<DummyStream, _> = SecureStream::new(s);
        let mut s = StreamAdapter::new(secure_s);

        // uncompressed
        let cmd = CommandRequest::new_hget("t1", "k1");
        s.send(cmd.clone()).await.unwrap();

        if let Some(Ok(s)) = s.next().await {
            assert_eq!(cmd, s);
        } else {
            assert!(false);
        }

        // compressed
        let cmd = CommandRequest::new_hget("t1", "k1".repeat(COMPRESSION_LIMIT));
        s.send(cmd.clone()).await.unwrap();

        if let Some(Ok(s)) = s.next().await {
            assert_eq!(cmd, s);
        } else {
            assert!(false);
        }
    }
}
