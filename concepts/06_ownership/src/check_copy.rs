fn is_copy<T: Copy>() {}

#[test]
fn types_impl_copy_trait() {
    is_copy::<bool>();
    is_copy::<char>();

    // all iXX and uXX, usize/isize, fXX implement Copy trait
    is_copy::<i8>();
    is_copy::<u64>();
    is_copy::<i64>();
    is_copy::<usize>();

    // function (actually a pointer) is Copy
    is_copy::<fn()>();

    // raw pointer is Copy
    is_copy::<*const String>();
    is_copy::<*mut String>();

    // immutable reference is Copy
    is_copy::<&[Vec<u8>]>();
    is_copy::<&String>();

    // array/tuple with values which is Copy is Copy
    is_copy::<[u8; 4]>();
    is_copy::<(&str, &str)>();
}

#[test]
fn types_not_impl_copy_trait() {
    // unsized or dynamic sized type is not Copy

    // is_copy::<str>();
    // is_copy::<[u8]>();
    // is_copy::<Vec<u8>>();
    // is_copy::<String>();

    // // mutable reference is not Copy
    // is_copy::<&mut String>();

    // // array/tuple with values that not Copy is not Copy
    // is_copy::<[Vec<u8>; 4]>();
    // is_copy::<(String, u32)>();
}

#[test]
fn test_heap_can_owe_stack() {
    let mut heap = vec![];
    let a = 1;
    heap.push(&a);
}
