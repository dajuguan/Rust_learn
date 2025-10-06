use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::{Arc, Mutex},
    thread, time,
};

#[test]
fn send() {
    let mut a = 2;
    thread::spawn(move || {
        // T: Send
        a = 3;
        println!("a in thread: {}", a);
    });

    println!("a: {}", a);
}

#[test]
fn sync() {
    // cell is not Sync, he set method on Cell<T> takes &self, so it requires only a shared reference &Cell<T>.
    // The method performs no synchronization, thus Cell cannot be Sync.
    let a = Cell::new(1); // Cell is Send, !Sync
    let handle = thread::spawn(|| {
        let c = a;
        println!("c:{:?}", c);
        // let b = a.get(); // sync产生是由于.get(&self)引入的
    });
    handle.join().unwrap();

    let a = Arc::new(RefCell::new(2)); // RefCell is also Send, !Sync
    let b = a.clone();

    // thread::spawn(move || {
    //     *b.borrow_mut() = 3;
    //     println!("b in thread: {}", *b.borrow());
    // });

    // If two threads attempt to clone Rcs that point to the same reference-counted value,
    // they might try to update the reference count at the same time,
    // which is undefined behavior because Rc doesn’t use atomic operations.
    let a = Rc::new(1); // Rc is !Send, !Sync
    let b = a.clone(); // sync 同样是由于clone引起的
    let handle = thread::spawn(move || {
        // thread 1 clone()
        // println!("a:{:?} ", a);  // a will race with main thread to update rc, so we can't Send it safely
    });

    let c = b.clone(); // main thread try to update rc
    handle.join().unwrap();

    // mutex gard is !Send, Sync
    // If a thread attempts to unlock a mutex that it has not locked or a mutex which is unlocked, undefined behavior results.
    // As a result these structures are not sendable as we must guarantee that,
    // the lock is only released on the same thread that acquired it.
    let a = Arc::new(Mutex::new(1));
    let b = a.clone();
    let mut g = b.lock().unwrap(); // MutexGuard is !Send, Sync
    let handle = thread::spawn(move || {
        // *g += 1;
        // println!("b = {:?}", g);
        // this thread can unlock the mutex in the main thread, when g is out of this scope.
        // if g hasn't been locked, then undefined behavior will happen.
    });

    let b = a.clone(); // Arc, Mutex is both Send, Sync
    let handle = thread::spawn(move || {
        // lock will use &self, so Arc and Mutex must be sync
        // thread spawn requires b to be send
        let c = b; // must be Send
        let mut g = c.lock().unwrap(); // Must be sync
                                       // 这里有同步机制，因此可以多个进程(引用获取值)对共享值运算，依然不会出错
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
fn test_onlysync() {
    let m = Arc::new(Mutex::new(5));
    let mut guard = m.lock().unwrap(); // 拿到 MutexGuard<i32>

    // 我们创建一个对 guard 的共享引用（&guard）
    let guard_ref = &guard;

    // Can't directly use spawn do join, because the other thread might has exit before handle joins.
    // that's why thread spawn's signature for bowrrowed object must have 'static lifetime.
    // So, guard might be dropped before handle thread.
    thread::scope(|s| {
        // 注意，这里不能直接 move guard 到线程里，但是可以把guard_ref move进来
        // 所以验证&mutexGaurd是 sync的
        let h1 = s.spawn(move || {
            println!("Read from thread1: {}", guard_ref);
        });
        let h2 = s.spawn(move || {
            println!("Read from thread2: {}", guard_ref);
        });
        h1.join().unwrap();
        h2.join().unwrap();
    });

    *guard = 2;

    println!("Main read: {}", guard);
}

#[test]
fn mutex() {
    let metrics: Arc<Mutex<HashMap<Cow<'static, str>, usize>>> =
        Arc::new(Mutex::new(HashMap::new()));
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
