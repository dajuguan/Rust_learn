fn accept_iterator<'a, T>(mut t: T) -> Option<&'a i32>
where
    T: Iterator<Item = &'a i32>,
{
    t.next()
}

#[test]
fn test_iter_trait() {
    use std::collections::VecDeque;

    let mut deque = VecDeque::new();

    deque.push_back(0);
    deque.push_back(1);

    println!("next:{:?}", accept_iterator(deque.iter()));
    println!("next:{:?}", accept_iterator(deque.iter()));

    let mut a = vec![1, 2, 3];

    println!("next:{:?}", accept_iterator(a.iter()));
    println!("next:{:?}", accept_iterator(a.iter()));
    a.push(1);
}
