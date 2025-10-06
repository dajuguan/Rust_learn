// mod closure;
// mod closure_trait;

#[no_mangle]
fn add_fn(x: i32) -> i32 {
    x + 1
}

fn main() {
    let a = 10;

    // 1. 函数指针调用
    let f: fn(i32) -> i32 = add_fn;
    let r1 = f(a);

    // 2. 闭包调用
    let closure = |x| x + 1;
    let r2 = closure(a);

    println!("r1 = {}, r2 = {}", r1, r2);
}
