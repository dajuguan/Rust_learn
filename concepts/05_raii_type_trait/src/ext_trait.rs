use std::ops::Add;

pub trait IteratorExt: Iterator where Self::Item: Clone + Add{
    fn my_next(&mut self) -> Option<<Self::Item as Add>::Output> {
        match self.next() {
            Some(item) => Some(item.clone() + item),
            _ => None
        }
    }
}

impl <T: ?Sized> IteratorExt for T where T: Iterator, T::Item: Clone + Add {}