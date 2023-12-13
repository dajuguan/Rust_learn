trait Pet {
    fn talk(&self);
}

struct Dog {
    name: String
}

impl Pet for Dog {
    fn talk(&self){
        println!("Dog says: {}", self.name);
    }
}

struct Cat {
    name: String
}

impl Pet for Cat {
    fn talk(&self){
        println!("Cat says: {}", self.name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let dog = Dog {
            name: "Wangwang".to_string()
        };
        let cat = Cat {
            name: "Miaomiao".to_string()
        };

        let pets: Vec<Box<dyn Pet>> = vec![Box::new(dog), Box::new(cat)];
        for pet in pets {
            pet.talk();
        }
    }
}

use std::{fs::File, io::Write};
#[test]
fn trait_obj_is_not_sized() {
    let mut f = File::create("/tmp/test_write_trait").unwrap();
    let w: &mut dyn Write = &mut f;
    w.write_all(b"hello ").unwrap();
    // let w1 = w.by_ref();
    // w1.write_all(b"world").unwrap();
}


use std::fmt::{Debug, Display};
use std::mem::transmute;

#[test]
fn print_vtable() {
    let s1 = String::from("hello world!");
    let s2 = String::from("goodbye world!");
    // Display/Debug trait object for s
    let w1: &dyn Display = &s1;
    let w2: &dyn Debug = &s1;

    // Display/Debug trait object for s1
    let w3: &dyn Display = &s2;
    let w4: &dyn Debug = &s2;

    // 强行把 triat object 转换成两个地址 (usize, usize)
    // 这是不安全的，所以是 unsafe
    let (addr1, vtable1): (usize, usize) = unsafe { transmute(w1) };
    let (addr2, vtable2): (usize, usize) = unsafe { transmute(w2) };
    let (addr3, vtable3): (usize, usize) = unsafe { transmute(w3) };
    let (addr4, vtable4): (usize, usize) = unsafe { transmute(w4) };

    // s 和 s1 在栈上的地址，以及 main 在 TEXT 段的地址
    println!(
        "s1: {:p}, s2: {:p}, main(): {:p}",
        &s1, &s2, print_vtable as *const ()
    );
    // trait object(s/Display) 的 ptr 地址和 vtable 地址
    println!("addr1: 0x{:x}, vtable1: 0x{:x}", addr1, vtable1);
    // trait object(s/Debug) 的 ptr 地址和 vtable 地址
    println!("addr2: 0x{:x}, vtable2: 0x{:x}", addr2, vtable2);

    // trait object(s1/Display) 的 ptr 地址和 vtable 地址
    println!("addr3: 0x{:x}, vtable3: 0x{:x}", addr3, vtable3);

    // trait object(s1/Display) 的 ptr 地址和 vtable 地址
    println!("addr4: 0x{:x}, vtable4: 0x{:x}", addr4, vtable4);

    // 指向同一个数据的 trait object 其 ptr 地址相同
    assert_eq!(addr1, addr2);
    assert_eq!(addr3, addr4);

    // 指向同一种类型的同一个 trait 的 vtable 地址相同
    // 这里都是 String + Display
    assert_eq!(vtable1, vtable3);
    // 这里都是 String + Debug
    assert_eq!(vtable2, vtable4);
}