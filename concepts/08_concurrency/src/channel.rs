use std::sync::mpsc;
use std::thread;

#[test]
fn test_channel() {
    let (tx, rx) = mpsc::channel();
    let tx1 = tx.clone();
    thread::spawn(move || {
        let val = 4;
        tx.send(val).unwrap();
    });

    thread::spawn(move || {
        let val = 3;
        tx1.send(val).unwrap();
    });

    for received in rx {
        println!("Got: {received}");
    }
}
