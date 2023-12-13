use std::io::{BufWriter, Write};
use std::net::TcpStream;


#[derive(Debug)]
struct MyWriter<W> {
    writer: W,
}

impl<W> MyWriter<W> where W: Write{
    pub fn new(writer: W) -> Self {
        Self {
            writer,
        }
    }
    pub fn write(&mut self, buf: &str) -> std::io::Result<()> {
        self.writer.write_all(buf.as_bytes())
    }
}

#[test]
fn tcp() {
    let stream = TcpStream::connect("127.0.0.1:8233").unwrap();
    let mut writer = MyWriter::new( BufWriter::new(stream));
    let res = writer.write("hello world!");
    println!("result: {:?}", res);
}