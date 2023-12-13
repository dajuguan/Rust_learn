
/// Ref: https://learn.lianglianglee.com/%e4%b8%93%e6%a0%8f/%e9%99%88%e5%a4%a9%20%c2%b7%20Rust%20%e7%bc%96%e7%a8%8b%e7%ac%ac%e4%b8%80%e8%af%be/11%20%e5%86%85%e5%ad%98%e7%ae%a1%e7%90%86%ef%bc%9a%e4%bb%8e%e5%88%9b%e5%bb%ba%e5%88%b0%e6%b6%88%e4%ba%a1%ef%bc%8c%e5%80%bc%e9%83%bd%e7%bb%8f%e5%8e%86%e4%ba%86%e4%bb%80%e4%b9%88%ef%bc%9f.md
use std::mem::{size_of, align_of};
struct S1 {
    a: u8,
    b: u16, // 16 bits aligned
    c: u8
}

struct S2 {
    a: u8,
    c: u32,  // 32 bits aligned
    b: u16,
}

#[test]
fn mem_align() {
    println!("size(bytes) of S1: {}, S2: {}", size_of::<S1>(), size_of::<S2>());
    println!("align(bytes) of S1: {}, S2: {}", align_of::<S1>(), align_of::<S2>());
    type Res =  Result<String, ()>;
    println!("size(bytes) of Result<String, ()>: {}", size_of::<Res>());
   
}

use std::collections::HashMap;


// Option 配合带有引用类型的数据结构，比如 &u8、Box、Vec、HashMap ，没有额外占用空间
// Rust 是这么处理的，我们知道，引用类型的第一个域是个指针，而指针是不可能等于 0 的，但是我们可以复用这个指针：
// 当其为 0 时，表示 None，否则是 Some，减少了内存占用，这是个非常巧妙的优化
enum E {
    A(f64),
    B(HashMap<String, String>),
    C(Result<Vec<u8>, String>),
}

// 这是一个声明宏，它会打印各种数据结构本身的大小，在 Option 中的大小，以及在 Result 中的大小
macro_rules! show_size {
    (header) => {
        println!(
            "{:<24} {:>4}    {}    {}",
            "Type", "T", "Option<T>", "Result<T, io::Error>"
        );
        println!("{}", "-".repeat(64));
    };
    ($t:ty) => {
        println!(
            "{:<24} {:4} {:8} {:12}",
            stringify!($t),
            size_of::<$t>(),
            size_of::<Option<$t>>(),
            size_of::<Result<$t, std::io::Error>>(),
        )
    };
}

#[test]
fn show_mem_align() {
    show_size!(header);
    show_size!(u8);
    show_size!(f64);
    show_size!(&u8);
    show_size!(Box<u8>);
    show_size!(&[u8]);

    show_size!(String);
    show_size!(Vec<u8>);
    show_size!(HashMap<String, String>);
    show_size!(E);
}