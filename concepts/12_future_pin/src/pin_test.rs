use std::{
    marker::PhantomPinned,
    ops::{Deref, DerefMut},
};

#[derive(Debug)]
struct SelfReferential {
    data: String,
    pointer: *const String,
}

impl SelfReferential {
    fn new(data: String) -> Self {
        let mut instance = SelfReferential {
            data,
            pointer: std::ptr::null(),
        };
        instance
    }

    fn init(&mut self) {
        self.pointer = &self.data as *const String;
    }

    fn get_data_via_pointer(&self, tag: &str) {
        println!(
            "[{}]: data:{:p}, ptr:{:p}, data:{}, data_ref:{}",
            tag,
            &self.data,
            self.pointer,
            self.data,
            unsafe { &*self.pointer },
        );
    }
}

/// a not correct implementation of Pin for derefMut
struct MyPin<Ptr> {
    ptr: Ptr,
}

impl<Ptr> MyPin<Ptr> {
    fn new(ptr: Ptr) -> Self {
        MyPin { ptr }
    }
}

impl<Ptr: Deref> Deref for MyPin<Ptr> {
    type Target = Ptr::Target;

    fn deref(&self) -> &Self::Target {
        &*self.ptr
    }
}

impl<Ptr: DerefMut> DerefMut for MyPin<Ptr> {
    // not correct, because derefmut allows the pinned inner value to  be moved.
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.ptr
    }
}

/// a correct implementation of Pin for derefMut
struct MyPinCorrect<Ptr> {
    ptr: Ptr,
}

impl<Ptr: Deref<Target: Unpin>> MyPinCorrect<Ptr> {
    fn new(ptr: Ptr) -> Self {
        MyPinCorrect { ptr }
    }
}

impl<Ptr: Deref> MyPinCorrect<Ptr> {
    unsafe fn new_unchecked(ptr: Ptr) -> Self {
        MyPinCorrect { ptr }
    }
}

impl<Ptr: Deref> Deref for MyPinCorrect<Ptr> {
    type Target = Ptr::Target;

    fn deref(&self) -> &Self::Target {
        &*self.ptr
    }
}

// only allowed unpin targets to be mutable refereced, because it doesn't care about pin guarantee.
impl<Ptr: DerefMut<Target: Unpin>> DerefMut for MyPinCorrect<Ptr> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.ptr
    }
}

impl<Ptr: DerefMut> MyPinCorrect<Ptr> {
    // as_mut is for resolving lifetime issue. Ptr has no lifetime while &mut T has
    // A bridge in MyPin<Ptr> -> MyPin<&'a mut T> -> &'a mut T
    fn as_mut(&mut self) -> MyPinCorrect<&mut Ptr::Target> {
        unsafe { MyPinCorrect::new_unchecked(&mut self.ptr) }
    }
}

impl<'a, T> MyPinCorrect<&'a mut T> {
    unsafe fn get_unchecked_mut_wrong(&mut self) -> &mut T {
        self.ptr
    }

    const unsafe fn get_unchecked_mut(self) -> &'a mut T {
        self.ptr
    }
}

#[derive(Debug)]
struct SelfReferentialPin {
    data: String,
    pointer: *const String,
    _pin: PhantomPinned, // impl !Unpin for this struct
}

impl SelfReferentialPin {
    fn new(data: String) -> Self {
        let mut instance = SelfReferentialPin {
            data,
            pointer: std::ptr::null(),
            _pin: PhantomPinned::default(),
        };
        instance.pointer = &instance.data as *const String;
        instance
    }

    fn init(&mut self) {
        self.pointer = &self.data as *const String;
    }

    fn get_data_via_pointer(&self, tag: &str) {
        println!(
            "[{}]: data:{:p}, ptr:{:p}, data:{}, data_ref:{}",
            tag,
            &self.data,
            self.pointer,
            self.data,
            unsafe { &*self.pointer },
        );
    }
}

#[cfg(test)]
mod tests {
    use std::{mem, pin::Pin};

    use super::*;

    fn move_create_issues() -> SelfReferential {
        let mut instance = SelfReferential::new(String::from("Hello, Pin!"));
        instance.init();
        instance.get_data_via_pointer("inst");
        instance
    }

    #[test]
    fn test_self_referential() {
        // instance is moved from move_create_issues{} to instance, must use function, or rust may optimize it away and the moved instance might not be gced, which cause the test passed uncorrectly.
        let instance = move_create_issues();
        instance.get_data_via_pointer("inst after move");
    }

    #[test]
    fn test_self_referential_mypin_wrong_new() {
        // instance is moved from move_create_issues{} to instance, must use function, or rust may optimize it away and the moved instance might not be gced, which cause the test passed uncorrectly.
        let mut instance = SelfReferentialPin::new(String::from("A!"));
        instance.init();
        {
            let pinned = MyPin::new(&instance);
            pinned.get_data_via_pointer("pinned");
        }

        // actually, we shouldn't be able to move it after pinning, but MyPin doesn't prevent it.
        let moved_instance = instance;
    }

    #[test]
    fn test_self_referential_mypin_wrong_derefmut() {
        // instance is moved from move_create_issues{} to instance, must use function, or rust may optimize it away and the moved instance might not be gced, which cause the test passed uncorrectly.
        let mut instance = SelfReferentialPin::new(String::from("A!"));
        instance.init();
        let instance = MyPin::new(&mut instance);
        instance.get_data_via_pointer("inst");
        let mut ins_a = instance;
        // nothing changed, because we can only move MyPin (ptr to SelfReferential), not the inner SelfReferential which is shadowed by MyPin instance.
        ins_a.get_data_via_pointer("inst after move to a");

        let mut ins_b = SelfReferentialPin::new(String::from("B!"));
        ins_b.init();
        let mut ins_b = MyPin::new(&mut ins_b);
        ins_b.get_data_via_pointer("inst b");
        std::mem::swap(&mut *ins_a, &mut *ins_b);
        // Not correct!
        ins_a.get_data_via_pointer("inst a after derefmut swap");
        ins_b.get_data_via_pointer("inst b after derefmut swap");
    }

    #[test]
    fn test_self_referential_mypin_correct_ub() {
        // instance is moved from move_create_issues{} to instance, must use function, or rust may optimize it away and the moved instance might not be gced, which cause the test passed uncorrectly.
        let mut instance = SelfReferentialPin::new(String::from("A!"));
        instance.init();
        let instance = unsafe { MyPinCorrect::new_unchecked(&mut instance) };
        instance.get_data_via_pointer("inst");
        let mut ins_a = instance;
        // nothing changed, because we can only move MyPinCorrect (ptr to SelfReferential), not the inner SelfReferential which is shadowed by MyPinCorrect instance.
        ins_a.get_data_via_pointer("inst after move to a");

        let mut ins_b = SelfReferentialPin::new(String::from("B!"));
        ins_b.init();
        let mut ins_b = unsafe { MyPinCorrect::new_unchecked(&mut ins_b) };
        ins_b.get_data_via_pointer("inst b");

        let mut a_mut = ins_a.as_mut();
        let mut b_mut = ins_b.as_mut();
        // from semantics, a_mut(注意视角是a_mut，而不是inst_a) is moved in unsafe which violates the pin guarantee.
        unsafe {
            // UB(undefined behavior)
            let a_mut = a_mut.get_unchecked_mut_wrong();
            let b_mut = b_mut.get_unchecked_mut_wrong();
            std::mem::swap(a_mut, b_mut);
        };

        // 从语义上来说，a_mut的内容不允许被改变，因为a_mut被pin住了，但是此处我们获取了a_mut的可变引用，并改变了a的内容，此时a_mut的pin保证被破坏了(而a_mut的所有者可能还不知道这回事儿，以为）。
        a_mut.get_data_via_pointer("inst after unsafe get mut");

        // from semantics, ins_a as_mut create a new MyPinCorrect<&mut T>, so the previous ins_a still holds the promise that it won't be moved, it's the new MyPinCorrect that moved the value.
        unsafe {
            // UB
            let a_mut = a_mut.get_unchecked_mut();
            let b_mut = b_mut.get_unchecked_mut();
            std::mem::swap(a_mut, b_mut);
        };
        // 由于a_mut被move了，所以不会出现为定义行为，a_mut的所有者知道Pinned的值被改变了。
        // a_mut.get_data_via_pointer("inst after unsafe get mut");

        ins_a.get_data_via_pointer("inst a after derefmut swap");
        ins_b.get_data_via_pointer("inst b after derefmut swap");
    }

    #[test]
    fn test_self_referential_pin_ub() {
        // instance is moved from move_create_issues{} to instance, must use function, or rust may optimize it away and the moved instance might not be gced, which cause the test passed uncorrectly.
        let mut instance = SelfReferentialPin::new(String::from("A!"));
        instance.init();
        let instance = unsafe { Pin::new_unchecked(&mut instance) };
        instance.get_data_via_pointer("inst");
        let mut ins_a = instance;
        ins_a.get_data_via_pointer("inst after move to a");

        let mut ins_b = SelfReferentialPin::new(String::from("B!"));
        ins_b.init();
        let mut ins_b = unsafe { Pin::new_unchecked(&mut ins_b) };
        ins_b.get_data_via_pointer("inst b");

        let a_mut = ins_a.as_mut();
        let b_mut = ins_b.as_mut();

        // This function is unsafe. You must guarantee that you will never move the data out of the mutable reference you receive when you call this function,
        // so that the invariants on the Pin type can be upheld.
        unsafe {
            // UB
            // 即使是拿到了Pin指向值的的可变引用Pin(一个新的pin)，也需要开发者自己保证不会移动被Pin住的值(因为这个Pin指向的值被move后，其他相关Pin的实例也会受到影响)，否则会违反Pin的语义。
            let a_mut = a_mut.get_unchecked_mut();
            let b_mut = b_mut.get_unchecked_mut();
            std::mem::swap(a_mut, b_mut);
        };

        ins_a.get_data_via_pointer("inst a after derefmut swap");
        ins_b.get_data_via_pointer("inst b after derefmut swap");
    }
}
