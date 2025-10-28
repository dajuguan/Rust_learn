pub use crate::either::Either::{Left, Right};

#[derive(Debug)]
pub enum Either<L, R> {
    Left(Option<L>),
    Right(R),
}

#[test]
fn test_either() {
    let x = 3;
    let mut res = match x {
        3 => Left(Some(0..3)),
        _ => Right(10),
    };

    // partial move res
    match res {
        Left(ref mut x) => println!("left:{:?}", x.take()),
        Left(None) => println!("left:None"),
        Right(x) => println!("right:{x:?}"),
    }

    println!("res after move:{res:?}")
}
