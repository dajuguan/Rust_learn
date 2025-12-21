mod async_sleep;
mod executor;
use std::time::Duration;

use async_sleep::*;
use executor::*;

fn main() {
    let (executor, spawner) = new_executor_spawner();

    spawner.spawn(async {
        println!("start sleep");
        AsyncSleep::new(Duration::from_millis(2000)).await;
        println!("done!")
    });

    // Drop the spawner so that our executor knows it is finished and won't
    // receive more incoming tasks to run.
    drop(spawner);

    executor.run();
}
