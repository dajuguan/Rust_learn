pub trait Encoder {
    fn encode(&self) -> Vec<u8>;
}

pub struct Event<Id, Data> {
    id: Id,
    data: Data
}

impl<Id, Data> Event<Id, Data>
where Id: Encoder, Data: Encoder {
    pub fn new(id: Id, data: Data) -> Event<Id, Data> {
        Event { id, data }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut result = self.id.encode();
        result.extend(&self.data.encode());
        result
    }
}

impl Encoder for u32 {
    fn encode(&self) -> Vec<u8> {
        vec![1]
    }
}

impl Encoder for String {
    fn encode(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let event = Event::new(1, "hello".to_string());
        println!("result: {:?}", event.encode());
    }
}
