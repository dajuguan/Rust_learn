// associate type trait
pub trait AssociateTypeTrait {
    fn height(&self) -> i32;
}

// composite type
pub trait CompositeTrait {
    type AssociatedType: AssociateTypeTrait;
    fn all(&self) -> &Self::AssociatedType;
}

pub struct MyAssociatedType;
impl AssociateTypeTrait for MyAssociatedType {
    fn height(&self) -> i32 {
        42
    }
}

// My struct
pub struct MyStruct<B = MyAssociatedType> {
    associate_type_obj: B,
}

impl<B: AssociateTypeTrait> CompositeTrait for MyStruct<B> {
    type AssociatedType = B;

    fn all(&self) -> &Self::AssociatedType {
        &self.associate_type_obj
    }
}

#[cfg(test)]
mod tests {
    use super::{AssociateTypeTrait, CompositeTrait, MyAssociatedType, MyStruct};
    #[test]
    fn test_associated_type() {
        let t = MyStruct {
            associate_type_obj: MyAssociatedType,
        };

        // 返回的是关联类型，而不是trait类型
        let at = t.all();
        assert_eq!(at.height(), 42);
    }
}
