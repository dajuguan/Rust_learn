# Design Requirements
client send req to a kv-server, which can accept CRUD cmd and return response.

- 消息格式: protolbuf(序列化/反序列化)
- client server通讯: stream (TCP, TLS)
    - Server区分消息: 采用frame_id
- Dispatcher: execute(self, request) -> Resp
- Storage:
    - get
    - set
    - getIter
- Notification
    - onGetRequest
    - onGetRequestMut
    - onSendReq
    - onSendRespMut
- Pub/Sub
    - stream => yamux stream
    - topic -> subscriberId
    - Server: subscriberId -> sender
    - Client: 如何识别是哪种MSG？

- 通过分析发现数据结构的类型是dispatcher/storage共同依赖的，所以需要先定义好数据结构