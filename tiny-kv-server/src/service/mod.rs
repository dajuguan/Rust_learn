mod cmd_service;

use crate::{CommandRequest, CommandResponse, Storage, Value, command_request::RequestData};

pub trait CommandService {
    fn execute(self, store: &impl Storage<String, Value>) -> CommandResponse;
}

pub fn dispatch(cmd_req: CommandRequest, store: &impl Storage<String, Value>) -> CommandResponse {
    match cmd_req.request_data.unwrap() {
        RequestData::Hget(params) => params.execute(store),
        RequestData::Hset(params) => params.execute(store),
        _ => CommandResponse::default(),
    }
}

#[cfg(test)]
pub fn assert_res_ok(res: &CommandResponse, values: &[Value], pairs: &[crate::Kvpair]) {
    use crate::StatusCode;

    let mut sorted_pairs = res.pairs.clone();
    sorted_pairs.sort_by(|a, b| a.partial_cmp(b).unwrap());

    assert_eq!(res.status, StatusCode::Ok.into());
    assert_eq!(res.message, "");
    assert_eq!(res.values, values);
    assert_eq!(sorted_pairs, pairs);
}
