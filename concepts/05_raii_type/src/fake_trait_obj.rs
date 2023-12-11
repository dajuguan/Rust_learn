pub struct TraitObj {
    pub data: *mut(),
    // pub vtable: *mut()
    pub vtable: *mut()
}

type Method = fn(*const()) -> String;

pub struct FooVtable {
    destructor: fn(*mut()),
    size: usize,
    align: usize,
    pub method: Method
}


// u8
fn call_method_u8(x: *const()) -> String {
    let byte: &u8 = unsafe {
        &*(x as *const u8)
    };
    byte.to_string()
}

static Foo_for_u8_vtable: FooVtable = FooVtable {
    destructor: |arg| {},
    size: 1,
    align: 1,
    method: call_method_u8 as Method
};


// string
fn call_method_string(x: *const()) -> String {
    let string: &String = unsafe {
        &*(x as *const String)
    }; 
    string.to_string()
}

static Foo_for_string_vtable: FooVtable = FooVtable {
    destructor: |arg| {},
    size: 24,
    align: 8,
    method: call_method_string as Method
};


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let mut s = "foo".to_string();
        let mut u: u8 = 1;

        let s_trait_obj = TraitObj {
            data: &mut s as *mut String as *mut(),
            vtable: &Foo_for_string_vtable as *const FooVtable as *mut()
        };

        let u_trait_obj = TraitObj {
            data: &mut u as *mut u8  as *mut(),   
            vtable: &Foo_for_u8_vtable as *const FooVtable as *mut()
        };
        unsafe {
            let foo_vtable: &FooVtable = &*(s_trait_obj.vtable as *const FooVtable);
            let res = (foo_vtable.method)(s_trait_obj.data);
            println!("res: {}", res);

            let foo_vtable: &FooVtable = &*(u_trait_obj.vtable as *const FooVtable);
            let res = (foo_vtable.method)(u_trait_obj.data);
            println!("res: {}", res);
        }
        
    }
}