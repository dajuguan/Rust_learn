use std::io::{Read, Write};

use prost::{
    Message,
    bytes::{Buf, BufMut, BytesMut},
};

use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::{CommandRequest, CommandResponse, KvError, debug};

const MAX_FRAME_SIZE: usize = 2 * 1024 * 1024 * 1024; // 2 GB
const HEADER_LEN: usize = 4;
const COMPRESSION_BIT: usize = 1 << 31;
// if payload size > 1436 bytes, compress it
#[cfg(not(test))]
const COMPRESSION_LIMIT: usize = 1436;
#[cfg(test)]
pub const COMPRESSION_LIMIT: usize = 12;

pub trait FrameCoder
where
    Self: Sized + Message + Default,
{
    fn decode_frame(buf: &mut BytesMut) -> Result<Self, KvError> {
        let header = buf.get_u32() as usize;
        let (len, compressed) = decode_header(header);
        if compressed {
            let mut decoder = GzDecoder::new(&buf[..len]);
            let mut decoded = Vec::new();
            decoder.read_to_end(&mut decoded)?;
            buf.advance(len);
            Ok(Self::decode(&decoded[..])?)
        } else {
            let msg = Self::decode(&buf[..len])?;
            buf.advance(len);
            Ok(msg)
        }
    }

    fn encoded_frame(&self, buf: &mut BytesMut) -> Result<(), KvError> {
        let len = self.encoded_len();
        if len > MAX_FRAME_SIZE {
            return Err(KvError::FrameSizeError);
        }

        buf.put_u32(len as u32);

        if len > COMPRESSION_LIMIT {
            let mut data = Vec::with_capacity(len);
            self.encode(&mut data)?;
            // use split off, so encoder will write the same slab. Thus, we can merge the compressed data with zero copy.
            let payload = buf.split_off(HEADER_LEN);
            buf.clear();

            let mut encoder = GzEncoder::new(payload.writer(), Compression::default());
            encoder.write_all(&data[..])?;

            let payload = encoder.finish()?.into_inner();

            debug!("Compress a frame: size {}({})", len, payload.len());
            // override len with compressed len
            buf.put_u32((payload.len() | COMPRESSION_BIT) as _);
            // merge the underling payload
            buf.unsplit(payload);
            Ok(())
        } else {
            Ok(self.encode(buf)?)
        }

        // unimplemented!()
    }
}

impl FrameCoder for CommandRequest {}
impl FrameCoder for CommandResponse {}

fn decode_header(header: usize) -> (usize, bool) {
    let len = header & !COMPRESSION_BIT;
    let compressed = (header & COMPRESSION_BIT) == COMPRESSION_BIT;
    (len, compressed)
}
pub async fn read_frame<S>(stream: &mut S, buf: &mut BytesMut) -> Result<(), KvError>
where
    S: AsyncRead + Unpin,
{
    let header = stream.read_u32().await? as usize;
    let (len, _) = decode_header(header);
    buf.reserve(HEADER_LEN + len);
    buf.put_u32(header as _);
    // by set buf's len before, then call read_exact
    unsafe { buf.advance_mut(len) };
    stream.read_exact(&mut buf[HEADER_LEN..]).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use prost::bytes::BytesMut;

    use crate::{
        CommandRequest, CommandResponse,
        networking::frame::{FrameCoder, decode_header, read_frame},
        utils::DummyStream,
    };

    #[test]
    fn test_cmd_request_encode_decode_should_work() {
        // uncompressed
        let req = CommandRequest::new_hget("t1", "k1");
        let mut buf = BytesMut::new();
        req.encoded_frame(&mut buf).unwrap();

        assert!(!is_compressed(&buf));
        let decoded = CommandRequest::decode_frame(&mut buf);
        assert_eq!(decoded.unwrap(), req);

        // compressed
        let req = CommandRequest::new_hget("table1", "key12");
        let mut buf = BytesMut::new();
        req.encoded_frame(&mut buf).unwrap();

        assert!(is_compressed(&buf));
        let decoded = CommandRequest::decode_frame(&mut buf);
        assert_eq!(decoded.unwrap(), req);
    }

    #[test]
    fn test_cmd_response_encode_decode_should_work() {
        // uncompressed
        let res = CommandResponse::default();
        let mut buf = BytesMut::new();
        res.encoded_frame(&mut buf).unwrap();

        assert!(!is_compressed(&buf));
        let decoded = CommandResponse::decode_frame(&mut buf);
        assert_eq!(decoded.unwrap(), res);

        // compressed
        let mut res = CommandResponse::default();
        res.message = "large command".to_string();
        let mut buf = BytesMut::new();
        res.encoded_frame(&mut buf).unwrap();

        assert!(is_compressed(&buf));
        let decoded = CommandResponse::decode_frame(&mut buf);
        assert_eq!(decoded.unwrap(), res);
    }

    fn is_compressed(buf: &BytesMut) -> bool {
        let v = &buf[..1];
        v[0] >> 7 == 1
    }

    #[test]
    fn test_decode_header() {
        let header = 0x80000002 as usize;
        let (len, compressed) = decode_header(header);
        assert_eq!(len, 2);
        assert!(compressed);

        let header = 0x1000000f as usize;
        let (len, compressed) = decode_header(header);
        assert_eq!(len, 0x1000000f);
        assert!(!compressed);
    }

    #[tokio::test]
    async fn test_read_frame() {
        let cmd = CommandRequest::new_hget("t1", "k1");
        let mut buf = BytesMut::new();
        cmd.encoded_frame(&mut buf).unwrap();

        let mut s = DummyStream { buf: buf };
        let mut buf = BytesMut::new();
        read_frame(&mut s, &mut buf).await.unwrap();

        let decoded_cmd = CommandRequest::decode_frame(&mut buf).unwrap();
        assert_eq!(cmd, decoded_cmd);
    }
}
