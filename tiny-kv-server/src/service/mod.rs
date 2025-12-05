use crate::{CommandRequest, CommandResponse, Hget, Storage, Value, command_request::RequestData};

mod cmd_service;
pub trait CommandService {
    fn execute(self, store: &impl Storage<String, Value>) -> CommandResponse;
}

pub fn dispatch(cmdReq: CommandRequest, store: &impl Storage<String, Value>) -> CommandResponse {
    match cmdReq.request_data.unwrap() {
        RequestData::Hget(params) => params.execute(store),
        RequestData::Hset(params) => params.execute(store),
        _ => CommandResponse::default(),
    };

    CommandResponse::default()
}
