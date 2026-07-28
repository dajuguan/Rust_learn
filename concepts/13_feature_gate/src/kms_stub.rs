pub struct KmsSigner;

impl KmsSigner {
    pub fn new() -> Self {
        panic!("kms feature is disabled");
    }

    pub fn sign(&self, _: &[u8]) -> Vec<u8> {
        panic!("kms feature is disabled");
    }
}