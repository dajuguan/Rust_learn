use std::marker::PhantomPinned;

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

    fn get_data_via_pointer(&self) {
        println!(
            "data:{:p}, ptr:{:p}, data:{}, data_ref:{}",
            &self.data,
            self.pointer,
            self.data,
            unsafe { &*self.pointer },
        );
    }
}

#[derive(Debug)]
struct SelfReferentialPin {
    data: String,
    pointer: *const String,
    _pin: PhantomPinned,
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

    fn get_data_via_pointer(&self) {
        println!(
            "data:{:p}, ptr:{:p}, data:{}, data_ref:{}",
            &self.data,
            self.pointer,
            self.data,
            unsafe { &*self.pointer },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn move_create_issues() -> SelfReferential {
        let mut instance = SelfReferential::new(String::from("Hello, Pin!"));
        instance.init();
        instance.get_data_via_pointer();
        instance
    }

    #[test]
    fn test_self_referential() {
        // instance is moved from move_create_issues{} to instance, must use function, or rust may optimize it away and the moved instance might not be gced, which cause the test passed uncorrectly.
        let instance = move_create_issues();
        instance.get_data_via_pointer();
    }
}
