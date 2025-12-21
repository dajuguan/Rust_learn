use std::{
    sync::{Arc, Mutex},
    task::{Poll, Waker},
    thread::{self, sleep},
    time::Duration,
};
pub struct AsyncSleep {
    shared: Arc<Mutex<SharedState>>,
}

#[derive(Default)]
struct SharedState {
    // if completed, we'll call waker
    completed: bool,
    waker: Option<Waker>,
}

impl Future for AsyncSleep {
    type Output = ();
    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let mut shard_guard = self.shared.lock().unwrap();
        if shard_guard.completed {
            Poll::Ready(())
        } else {
            // save waker for Self to wake and poll itself again
            shard_guard.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

impl AsyncSleep {
    pub fn new(dur: Duration) -> Self {
        let shared = Arc::new(Mutex::new(SharedState::default()));

        // spawn a thread to sleep and wake after dur time.
        let shared_clone = shared.clone();
        thread::spawn(move || {
            sleep(dur);
            let mut shared_guard = shared_clone.lock().unwrap();
            shared_guard.completed = true;
            if let Some(waker) = shared_guard.waker.take() {
                waker.wake();
            };
        });

        AsyncSleep { shared }
    }
}
