/// 模拟一个私有 KMS 客户端。
pub struct Client;

impl Client {
    pub fn new() -> Self {
        Client
    }

    /// 假签名：把消息按字节取反，仅用于演示。
    pub fn sign(&self, msg: &[u8]) -> Vec<u8> {
        msg.iter().map(|b| !b).collect()
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}
