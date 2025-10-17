fn foo() {}

#[derive(Clone)]
struct Foo {
    data: i32,
}

#[test]
fn pointer() {
    let s = "hello".to_string();
    let data = Box::new(1);
    println!("RODATA: {:p}", "hello");
    println!("STACK: {:p}", &s);
    println!("HEAP: {:p}", &*s);
    println!("HEAP: {:p}, {:p}", data, &*data);
    println!("TEXT: {:p}", foo as *const ());
}

#[test]
fn pointer_box() {
    let foo = Foo { data: 0 };
    println!("STACK struct: {:p}", &foo);
    let fooBox = Box::new(foo.clone());
    println!("STACK box: {:p}", &fooBox);
    println!("HEAP box: {:p}", &*fooBox);

    let fooBox1 = Box::new(foo);
    println!("STACK box1: {:p}", &fooBox1);
    println!("HEAP box1: {:p}", &*fooBox1);

    let fooBox2 = Box::new(Foo { data: 1 });
    println!("STACK box2: {:p}", &fooBox2);
    println!("HEAP box2: {:p}", &*fooBox2);
}
