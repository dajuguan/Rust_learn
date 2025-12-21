use std::pin::Pin;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::task::{Context, Waker};
use std::{
    sync::mpsc::{Receiver, Sender},
    task::Wake,
};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

struct Task {
    fut: Mutex<Option<BoxFuture<'static, ()>>>,
    sender: Sender<Arc<Task>>,
}

pub struct Executor {
    task_receiver: Receiver<Arc<Task>>,
}

pub struct Spawner {
    task_sender: Sender<Arc<Task>>,
}

impl Wake for Task {
    fn wake(self: Arc<Self>) {
        self.sender.send(self.clone()).expect("failed to wake");
    }
}

impl Executor {
    pub fn run(&self) {
        while let Ok(task) = self.task_receiver.recv() {
            let mut fut_guard = task.fut.lock().unwrap();
            if let Some(mut fut) = fut_guard.take() {
                let arc_task = Arc::clone(&task);
                let waker = Waker::from(arc_task);
                let mut cx = Context::from_waker(&waker);
                if fut.as_mut().poll(&mut cx).is_pending() {
                    *fut_guard = Some(fut);
                }
            }
        }
    }
}

impl Spawner {
    pub fn spawn(&self, fut: impl Future<Output = ()> + 'static + Send) {
        let fut = Box::pin(fut);
        let task = Task {
            fut: Mutex::new(Some(fut)),
            sender: self.task_sender.clone(),
        };
        self.task_sender
            .send(Arc::new(task))
            .expect("fail to send task from spawner");
    }
}

pub fn new_executor_spawner() -> (Executor, Spawner) {
    let (tx, rx) = channel();
    (Executor { task_receiver: rx }, Spawner { task_sender: tx })
}
