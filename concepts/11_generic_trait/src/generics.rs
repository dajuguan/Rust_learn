use std::io::{BufWriter, Write};
use std::net::TcpStream;

#[derive(Debug)]
struct MyWriter<W> {
    writer: W,
}

impl<W: Write> MyWriter<W> {
    pub fn new(writer: W) -> Self {
        Self { writer: writer }
    }

    pub fn write(&mut self, buf: &str) -> std::io::Result<()> {
        self.writer.write_all(buf.as_bytes())
    }
}

#[test]
fn test_generic() {
    let addr = "127.0.0.1:7890";
    let stream = TcpStream::connect(addr).unwrap();
    let writer = BufWriter::new(stream);

    let mut writer = MyWriter::new(writer);
    writer.write("hello world!");
}
