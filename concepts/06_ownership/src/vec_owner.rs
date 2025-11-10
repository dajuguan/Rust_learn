#[test]
fn test_vec_move() {
    let mut str_vec = vec![String::from("1"), String::from("2")];
    let v1 = std::mem::take(&mut str_vec[1]);
    println!("moved v1:{:?}, vec at 1:{:?}", v1, str_vec[1]);
}
