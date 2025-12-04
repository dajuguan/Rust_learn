use crate::{CommandRequest, CommandResponse, Hget, Storage, command_request::RequestData};

mod cmd_service;
pub trait CommandService {
    fn execute(self, store: &impl Storage) -> CommandResponse;
}

pub fn dispatch(cmdReq: CommandRequest, store: &impl Storage) -> CommandResponse {
    match cmdReq.request_data.unwrap() {
        RequestData::Hget(param) => CommandResponse::default(),
        _ => CommandResponse::default(),
    };

    CommandResponse::default()
}
