use private_kms::Client;

pub struct KmsSigner {
    client: Client,
}

impl KmsSigner {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    pub fn sign(&self, msg: &[u8]) -> Vec<u8> {
        self.client.sign(msg)
    }
}