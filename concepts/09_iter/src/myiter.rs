use crate::iter_ext::WindowExt;

#[derive(Clone)]
struct MyVec<T>(Vec<T>);

struct MyVecIterator<T> {
    vec: MyVec<T>,
    pt: usize
}


impl<T: Copy> IntoIterator for MyVec<T> {
    type Item = T;
    type IntoIter = MyVecIterator<T>;
    fn into_iter(self) -> Self::IntoIter {
        MyVecIterator { vec: self, pt: 0 }
    }
}


impl<T: Copy> Iterator for MyVecIterator<T> {
    type Item = T;
    fn next(&mut self) -> Option<Self::Item> {
        self.pt = (self.pt + 2) % self.vec.0.len();
        Some(self.vec.0[self.pt])
    }
}

#[test]
fn test_iter() {
    let d = MyVec(vec![1,2,3,4,5,6,7,8]);
    let mut iter = d.clone().into_iter();

    println!("size of iter: {}", std::mem::size_of::<MyVecIterator<i32>>());

    for _ in 0..10 {
        println!("next: {:?}", iter.next());
    }

    let iter = d.into_iter();
    let mut iter = iter.window_count(2);
    for _ in 0..10 {
        println!("next: {:?}", iter.next());
    }
}