use iter_ext::WindowExt;

mod iter_ext;
mod myiter;

#[test]
fn test_window_iter_ext() {
    let a = vec![1,2,3,4,5,6,7,8];
    let iter = a.iter();
    let mut iter = iter.window_count(3);
    println!("window 3 next: {:?}", iter.next());
    println!("window 3 next: {:?}", iter.next());
    println!("window 3 next: {:?}", iter.next());
    println!("window 3 next: {:?}", iter.next());

}

fn main() {
    println!("Hello, world!");
}
