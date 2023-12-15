use std::mem::size_of_val;
#[test]
fn test() {
    let name = String::from("Tyr");
    let vec = vec!["Rust", "Elixir", "Javascript"];
    let v = &vec[..];
    let data = (1, 2, 3);  // 由于内存对其原因， 实际上闭包会多捕获 8 bytes
    // let data: i32 = 0;
    let c =  move || {
        println!("data: {:?}", data);
        println!("v: {:?}, name: {:?}", v, name.clone());
        println!("size of v: {}, name:{}, data: {}", size_of_val(&v), size_of_val(&name), size_of_val(&data));
    };
    println!("size of closure c before execute:{}", std::mem::size_of_val(&c));
    c();
    println!("size of closure c:{}", std::mem::size_of_val(&c));
    // println!("{}", name);   // name has been moved to closure
}