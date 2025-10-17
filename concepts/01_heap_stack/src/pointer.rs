fn foo() {}

#[derive(Clone)]
struct Foo {
    // rust will alloc 32 bytes (not 4 bytes) when using Box::new
    data: i32,
}

struct FooLarge {
    data: i64,
    s1: i128,
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
    let fooClone = foo.clone();
    println!(
        "STACK struct: foor: {:p}, foo clone: {:p}, size of fooPtr: {}",
        &foo,
        &fooClone,
        std::mem::size_of_val(&foo)
    );
    let fooBox = Box::new(fooClone);

    println!(
        "STACK box: {:p}, size of boxPtr: {}",
        &fooBox,
        std::mem::size_of_val(&fooBox)
    );
    println!("HEAP box: {:p}", &*fooBox);

    let fooBox1 = Box::new(foo);

    let fooBox2 = Box::new(Foo { data: 1 });
    let fooBox3 = Box::new(FooLarge { data: 1, s1: 2 });
    let fooBox4 = Box::new(FooLarge { data: 1, s1: 2 });
    let fooBox5 = Box::new(FooLarge { data: 1, s1: 2 });
    println!("STACK box1: {:p}", &fooBox1);
    println!("STACK box2: {:p}", &fooBox2);

    println!("HEAP box1: {:p}", &*fooBox1);
    println!("HEAP box2: {:p}", &*fooBox2);
    println!("HEAP box3: {:p}", &*fooBox3);
    println!("HEAP box4: {:p}", &*fooBox4);
    println!("HEAP box5: {:p}", &*fooBox5);
}
