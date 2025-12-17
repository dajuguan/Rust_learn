use std::sync::mpsc;
use std::thread;
use std::time::Duration;

#[derive(Debug)]
struct Task {
    id: usize,
}

// Stage A
fn stage_a(input: mpsc::Receiver<Task>, output: mpsc::Sender<Task>) {
    for task in input {
        println!("A start t{}", task.id);
        thread::sleep(Duration::from_millis(100));
        println!("A end   t{}", task.id);
        output.send(task).unwrap();
    }
}

// Stage B
fn stage_b(input: mpsc::Receiver<Task>, output: mpsc::Sender<Task>) {
    for task in input {
        println!("    B start t{}", task.id);
        thread::sleep(Duration::from_millis(100));
        println!("    B end   t{}", task.id);
        output.send(task).unwrap();
    }
}

// Stage C
fn stage_c(input: mpsc::Receiver<Task>) {
    for task in input {
        println!("        C start t{}", task.id);
        thread::sleep(Duration::from_millis(100));
        println!("        C end   t{}", task.id);
    }
}

#[cfg(test)]
#[test]
fn test_pipeline() {
    let (tx_a, rx_a) = mpsc::channel();
    let (tx_b, rx_b) = mpsc::channel();
    let (tx_c, rx_c) = mpsc::channel();

    thread::spawn(|| stage_a(rx_a, tx_b));
    thread::spawn(|| stage_b(rx_b, tx_c));
    thread::spawn(|| stage_c(rx_c));

    // send tasks
    for i in 1..=8 {
        tx_a.send(Task { id: i }).unwrap();
    }

    drop(tx_a); // close pipeline
    thread::sleep(Duration::from_secs(1));
}
