/*
'static is a reserved lifetime names. You might encounter it in two situations:
1. A reference with 'static lifetime: 
    'static indicates that the data pointed to by the reference lives for the remaining lifetime of the running program
     let s: &'static str = "hello world"; 
2. 'static as part of a trait bound:  it means the type does not contain any non-static references. 
    so reference types don't satisfy the 'static bound.
    fn generic<T>(x: T) where T: 'static {}
    T: 'static 要求 T 与上下文没有关联，在难以静态决定生存期的并发环境下很有用。
references: https://doc.rust-lang.org/rust-by-example/scope/lifetime/static_lifetime.html

 */


use std::fmt::Debug;

fn get_mem_location() -> (usize, usize) {
    let string = "hello";
    let pointer = string.as_ptr() as usize;
    let length = string.len();
    return (pointer, length);
}

fn get_str_at_location(pointer: usize, length: usize) -> &'static str {
    unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(pointer as *const u8, length)) }
}


fn print_it<T: Debug + 'static>(input: &T) {
    println!("static data type passed in is: {:?}", input);
}

#[allow(dead_code)]
#[derive(Debug)]
struct NonstaticData<'a> {
    a: u32,
    b: &'a i32
}

#[derive(Debug)]
struct Res {
    value: u32
}

pub struct Constainer<'a> {
    a: &'a str,
    cb: Option<Box<dyn Fn(&str) -> Res>>  // closure has static lifetime requirement
}

impl<'a> Constainer<'a> {
    fn new(a: &str) -> Constainer {
        Constainer { a, cb: None }
    }

    fn set(&mut self, cb: impl Fn(&str) -> Res + 'static) {
        self.cb = Some(Box::new(cb));
    }
    fn invoke_cb(&self) {
        if let Some(cb) = &self.cb {
            let res = cb("hello cb");
            println!("res from cb is: {:?}", res);
        }
    }
}

fn main() {
    // reference with static lifetime
    let (pt, len) = get_mem_location();
    let s = get_str_at_location(pt, len);
    println!("Len {} at 0x{}, has str: {}", pt, len, s);

    // static as part of the trait bound
    let t = 5;  // t does not contain any non-static references
    print_it(&t);

    //failed, because b doesn't have a static lifetime
    // let c = NonstaticData {a: 1, b:&t};
    // print_it(&c);

    // closure has static lifetime, 即不能包含任何非'static引用字段
    let mut c = Constainer::new("hello world");
    c.set(|val| {
        println!("val :{}", val);
        Res {value: 5}
    });

    c.invoke_cb();
}
