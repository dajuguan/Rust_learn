use std::sync::MutexGuard;
use std::sync::{Arc, Mutex};
use std::thread;

#[test]
fn fn_test_mutexguard_sync() {
    let m = Mutex::new(vec![1, 2, 3]);
    let guard = m.lock().unwrap(); // MutexGuard<Vec<i32>>

    thread::scope(|s| {
        for _ in 0..3 {
            let g_ref: &MutexGuard<Vec<i32>> = &guard;
            s.spawn(move || {
                // 只读访问
                println!("len = {}", g_ref.len());
            });
        }
    });
}
