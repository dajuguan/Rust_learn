use std::{fs::File, io::{Write, BufReader, Read}};

use serde::{Serialize,Deserialize};


#[derive(Debug, Serialize, Deserialize)]
struct Student {
    name: String
}

fn flex_serialize(s: &Student) -> Vec<u8> {
    flexbuffers::to_vec(s).unwrap()
}


#[test]
fn test_flexbuffers() {
    let s = Student {name: "test".to_string()};
    let serialized_s = flex_serialize(&s);

    println!("serialized_s: {:?}", serialized_s);

    let deserialized_s: Student = flexbuffers::from_buffer(&serialized_s.as_slice()).unwrap();
    println!("deserialized_s: {:?}", deserialized_s);
}

#[test]
fn test_flexbuffers_file() -> Result<(), Box<dyn std::error::Error>> {
    let s = Student {name: "test".to_string()};
    let serialized_s = flex_serialize(&s);
    // write
    {
        let mut fd = std::fs::File::create("data.buf").unwrap();
        fd.write_all(&serialized_s)?;
    }
    // load back
    {
        let fd: File = File::open("data.buf")?;
        let mut buffer_reader = BufReader::new(fd);
        let mut buf = Vec::new();
        buffer_reader.read_to_end(&mut buf)?;
        let deserialized_s: Student = flexbuffers::from_buffer(&buf.as_slice()).unwrap();
        println!("deserialized_s: {:?}", deserialized_s);

        std::fs::remove_file("data.buf")?;
    }

    Ok(())
}