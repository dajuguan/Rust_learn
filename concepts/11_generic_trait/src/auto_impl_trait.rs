use auto_impl::auto_impl;

#[auto_impl(&, &mut, Box)]
trait AutoTrait1 {
    fn storage(&self) -> usize;
}

fn func_accept_trait<T>(item: T) -> usize
where
    T: AutoTrait1,
{
    item.storage()
}

#[test]
fn test_auto_trait_types() {
    struct MyStruct {
        data: Vec<u8>,
    }

    impl AutoTrait1 for MyStruct {
        fn storage(&self) -> usize {
            self.data.len()
        }
    }

    let my_instance = MyStruct {
        data: vec![1, 2, 3, 4, 5],
    };

    // auto_impl will generate impls for &MyStruct, &mut MyStruct, Box<MyStruct>
    // impl AutoTrait1 for &MyStruct { ... }
    // impl AutoTrait1 for &mut MyStruct { ... }
    // impl AutoTrait1 for Box<MyStruct> { ... }
    assert_eq!(func_accept_trait(&my_instance), 5);
    assert_eq!(func_accept_trait(my_instance), 5);
}
