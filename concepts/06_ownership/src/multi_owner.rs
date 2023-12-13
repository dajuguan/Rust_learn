use std::{cell::{Cell, RefCell}, rc::Rc};

#[derive(Debug, Clone)]
struct Data {
    pub name: String,
}
#[test]
fn multi_owner() {
    let a = Cell::new(1);
    let b = a.clone();
    let c = a.get();
    a.set(3);
    b.set(3);
    println!("a: {}, b: {}, c: {}", a.get(), b.get(), c);

    // cell doesn't support none-copy data types for get method
    let data = Data {name: "cc".to_string()};
    let a = Cell::new(data.clone());
    let data1 = Data {name: "dd".to_string()};
    // let d =  a.get(); 

    let a = Rc::new(RefCell::new(data));
    {
        let mut b = a.borrow_mut();
        b.name = "ee".to_string();
    }

    let c = a.borrow();
    println!("c: {:?}", c); 
    
}