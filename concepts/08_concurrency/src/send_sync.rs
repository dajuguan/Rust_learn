use std::{thread, sync::{Arc, Mutex}, cell::RefCell, collections::HashMap, borrow::Cow, time};

#[test]
fn send() {
    let mut a = 2;
    thread::spawn(move || {
        a = 3;
        println!("a in thread: {}", a);
    });

    println!("a: {}", a);
}

#[test]
fn sync() {
    let a = Arc::new(RefCell::new(2));
    let b = a.clone();

    // thread::spawn(move || {
    //     *b.borrow_mut() = 3;
    //     println!("b in thread: {}", *b.borrow());
    // });

    let a = Arc::new(Mutex::new(1));
    let b = a.clone();
    let handle = thread::spawn(move || {
        let mut g = b.lock().unwrap();
        *g += 1;
        println!("b = {:?}", g);
    });

    {
        let mut g = a.lock().unwrap();
        *g += 1;
        println!("a in lock = {:?}", g);
    }

    println!("a = {:?}", a);

    handle.join().unwrap();
    println!("a after join= {:?}", a);
}

#[test]
fn mutex() {
    let metrics: Arc<Mutex<HashMap<Cow<'static, str>, usize>>> = Arc::new(Mutex::new(HashMap::new()));
    for _ in 0..32 {
        let m = metrics.clone();
        thread::spawn(move || {
            let mut g = m.lock().unwrap();
            let data = &mut *g;
            let entry = data.entry("hello".into()).or_insert(0);
            *entry += 1;
        });
    }

    thread::sleep(time::Duration::from_millis(100));
    println!("metrics: {:?}", metrics.lock().unwrap());
}