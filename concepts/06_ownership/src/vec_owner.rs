#[test]
fn test_vec_move() {
    let mut str_vec = vec![String::from("1"), String::from("2")];
    let v1 = std::mem::take(&mut str_vec[1]);
    println!("moved v1:{:?}, vec at 1:{:?}", v1, str_vec[1]);
}

#[test]
fn test_vec_addr_non_reallocation() {
    // 创建一个 vec 并填充一些元素
    let mut v = vec![1, 2, 3];

    // 获取 v 中第一个元素的地址
    let v_first_addr = &v[0] as *const i32;

    // 调用 std::mem::take 将 v 的内容转移到 taken
    let taken = std::mem::take(&mut v);

    // 获取 taken 中第一个元素的地址
    let taken_first_addr = &taken[0] as *const i32;

    // 输出地址
    println!("Address of before v's first element: {:?}", v_first_addr);
    println!("Address of taken's first element: {:?}", taken_first_addr);

    // 打印 taken 和 v 的内容:发现take没有重新分配内存
    println!("Taken: {:?}", taken); // [1, 2, 3]
    println!("v after take: {:?}", v); // []
}
