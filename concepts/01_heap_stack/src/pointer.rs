fn foo() {}

fn main() {
    let s = "hello".to_string();
    let data = Box::new(1);
    println!("RODATA: {:p}", "hello");
    println!("STACK: {:p}", &s);
    println!("HEAP: {:p}", &*s);
    println!("HEAP: {:p}, {:p}", data, &*data);
    println!("TEXT: {:p}", foo as *const());
}