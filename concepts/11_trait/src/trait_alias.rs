pub trait Trait1 {
    type AssocType1;
    fn method1(&self) -> Self::AssocType1;
}

pub trait Trait2 {
    type AssocType2;
    fn method2(&self) -> Self::AssocType2;
}

pub trait CombinedTrait: Trait1 + Trait2 {}

// Note: Rust does not currently support trait aliases directly.
impl<T> CombinedTrait for T where T: Trait1 + Trait2 {}
