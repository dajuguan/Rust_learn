mod raii;
mod generic;
mod trait_obj;
mod ext_trait;
mod fake_trait_obj;

use ext_trait::IteratorExt;
fn main() {
    let mut a = vec![1,2,3].into_iter();
    let next = a.my_next();
    assert!(next == Some(2));
    let next = a.my_next();
    assert!(next == Some(4));
}
