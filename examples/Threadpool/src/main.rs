use std::{
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, Sender},
    },
    thread::{self, JoinHandle},
};

struct Threadpool {
    // must use Option to take partial ownership
    sender: Option<Sender<Job>>,
    workers: Vec<Worker>,
}

impl Threadpool {
    pub fn new(size: usize) -> Self {
        let mut workers = vec![];
        let (tx, rx) = mpsc::channel();
        let receiver = Arc::new(Mutex::new(rx));
        for i in 0..size {
            let worker = Worker::new(i, receiver.clone());
            workers.push(worker);
        }

        return Threadpool {
            sender: Some(tx),
            workers,
        };
    }
    pub fn execute<T: FnOnce() + Send + 'static>(&self, f: T) {
        let job = Box::new(f);
        self.sender.as_ref().unwrap().send(job).unwrap();
    }
}

impl Drop for Threadpool {
    fn drop(&mut self) {
        drop(self.sender.take());
        for worker in &mut self.workers {
            if let Some(handle) = worker.handle.take() {
                handle.join().unwrap();
                println!("exit worker: {:?}", worker.id);
            }
        }
    }
}

struct Worker {
    id: usize,
    handle: Option<JoinHandle<()>>,
}

// Every closure is a specific type in Rust, so we use pointer to closure.
type Job = Box<dyn FnOnce() + Send + 'static>;

impl Worker {
    fn new<F>(id: usize, rx: Arc<Mutex<Receiver<F>>>) -> Self
    where
        F: FnOnce() + Send + 'static,
    {
        let handle = thread::spawn(move || {
            loop {
                let msg = rx.lock().unwrap().recv();
                match msg {
                    Ok(f) => {
                        println!("job received from worker: {id}");
                        f()
                    }
                    Err(_) => {
                        println!("error, worker {id} stops");
                        break;
                    }
                }
            }
        });

        return Worker {
            id,
            handle: Some(handle),
        };
    }
}

fn main() {
    let tp = Threadpool::new(4);
    let fib = || println!("run func1");
    tp.execute(fib);
    let fib1 = || println!("run func2");
    tp.execute(fib1);
}
