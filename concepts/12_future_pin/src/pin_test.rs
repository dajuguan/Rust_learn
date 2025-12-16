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
    // get_unchecked_mut_wrong only provides a reference that lives for as long as the borrow of the Pin, not the lifetime of the reference contained in the Pin.
    unsafe fn get_unchecked_mut_wrong_lifetime(&mut self) -> &mut T {
        self.ptr
    }

    // This method allows turning the Pin into a reference with the same lifetime as the reference it wraps.
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
    fn test_self_referential_mypin_correct_shorter_lifetime() {
        // instance is moved from move_create_issues{} to instance, must use function, or rust may optimize it away and the moved instance might not be gced, which cause the test passed uncorrectly.
        let mut instance = SelfReferentialPin::new(String::from("A!"));
        instance.init();
        let mut instance = unsafe { MyPinCorrect::new_unchecked(&mut instance) };

        let a_mut = unsafe {
            // UB(undefined behavior)
            let mut a_mut = instance.as_mut();
            // a mut is only as only as instance.as_mut() borrow, so the returned reference can't outlive it. Thus, rust doesn't allow it.
            let a_mut = a_mut.get_unchecked_mut_wrong_lifetime();
            a_mut
        };

        a_mut.get_data_via_pointer("inst after unsafe get mut");
    }

    #[test]
    fn test_self_referential_mypin_correct_correct_lifetime() {
        // instance is moved from move_create_issues{} to instance, must use function, or rust may optimize it away and the moved instance might not be gced, which cause the test passed uncorrectly.
        let mut instance = SelfReferentialPin::new(String::from("A!"));
        instance.init();
        let mut instance = unsafe { MyPinCorrect::new_unchecked(&mut instance) };

        let a_mut = unsafe {
            // UB(undefined behavior)
            let mut a_mut = instance.as_mut();
            let a_mut = a_mut.get_unchecked_mut();
            a_mut
        };

        a_mut.get_data_via_pointer("inst after unsafe get mut");
    }
}

#[cfg(test)]
#[test]
fn test_break_pin_promise() {
    use std::mem;
    use std::pin::Pin;
    struct SelfRef {
        data: String,
        ptr: *const String,
    }

    impl SelfRef {
        fn new(s: &str) -> Self {
            let mut v = Self {
                data: s.to_string(),
                ptr: std::ptr::null(),
            };
            v.ptr = &v.data;
            v
        }
    }

    let mut v = SelfRef::new("hello");
    println!("Before move, ptr points to: {}", unsafe { &*v.ptr });
    let pinned = unsafe { Pin::new_unchecked(&mut v) };
    unsafe {
        // move out SelfRef
        let _moved = mem::replace(pinned.get_unchecked_mut(), SelfRef::new("boom"));
    }

    println!("After move, ptr points to: {}", unsafe { &*v.ptr });
}
