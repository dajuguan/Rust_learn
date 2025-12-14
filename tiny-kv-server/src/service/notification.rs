pub trait Notify<T> {
    fn notify(&self, cmd: &T);
}

impl<T> Notify<T> for Vec<fn(&T)> {
    fn notify(&self, cmd: &T) {
        for notifier in self.iter() {
            notifier(cmd);
        }
    }
}

pub trait NotifyMut<T> {
    fn notify(&self, cmd: &mut T);
}

impl<T> NotifyMut<T> for Vec<fn(&mut T)> {
    fn notify(&self, cmd: &mut T) {
        for notifier in self.iter() {
            notifier(cmd);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{CommandRequest, CommandResponse};
    use crate::{assert_res_ok, service::ServiceInner};

    use super::*;
    #[test]
    fn test_notify_should_work() {
        let mut notifiers: Vec<fn(&CommandRequest)> = vec![];
        notifiers.push(|_cmd: &CommandRequest| {
            println!("get req:{:?}", _cmd);
        });

        notifiers.push(|_cmd: &CommandRequest| {
            println!("get req1:{:?}", _cmd);
        });

        let req = CommandRequest::new_hget("t1", "k1");
        notifiers.notify(&req);
    }

    #[test]
    fn test_service_notify_should_work() {
        use crate::{MemStore, Service, Value};

        let inner = ServiceInner::new(MemStore::default())
            .fn_on_req(|cmd| {
                println!("get req:{:?}", cmd);
            })
            .fn_on_exe(|resp| {
                println!("get resp:{:?}", resp);
            })
            .fn_on_before_send(|resp| {
                resp.message = "modified before send".to_string();
                println!("change resp:{:?}", resp);
            });

        let service: Service<String, Value, _> = inner.into();
        let cmd = CommandRequest::new_hset("t1", "k1", "v1".into());
        let res = service.execute(cmd);
        assert_res_ok(&res, &[Value::default()], &[]);
        assert_eq!(res.message, "modified before send");
    }
}
