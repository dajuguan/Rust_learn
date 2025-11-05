trait Human {
    fn speak(&self);
}

#[derive(Clone, Copy)]
struct Man {}

impl Human for Man {
    fn speak(&self) {
        println!("man speak");
    }
}

struct WoMan {}

impl Human for WoMan {
    fn speak(&self) {
        println!("woman speak");
    }
}

fn accept_generic_vec(v: Vec<impl Human>) {
    for item in v.iter() {
        item.speak();
    }
}

// the above is the abbreviation of generic type,
fn accept_generic_vec_t<T: Human>(v: Vec<T>) {
    for item in v.iter() {
        item.speak();
    }
}

fn accept_trait_obj_vec(v: Vec<&dyn Human>) {
    for item in v.iter() {
        item.speak();
    }
}

// trait object is a type, not a trait, so it can't be used as trait bound or generic type!
// fn accept_trait_obj_vec_t<T: &dyn Human>(v: Vec<T>) {
//     for item in v.iter() {
//         item.speak();
//     }
// }

#[test]
fn test_duck_type() {
    let m = Man {};
    let w = WoMan {};
    let peoples: Vec<&dyn Human> = vec![&m, &w];
    for p in peoples {
        p.speak();
    }
}

#[test]
fn test_duck_type_fn() {
    let m = Man {};
    let w = WoMan {};
    let peoples: Vec<&dyn Human> = vec![&m, &w];
    // accept_generic_vec(peoples);   // can't accept generics
    accept_trait_obj_vec(peoples);

    let peoples = vec![m, m];
    accept_generic_vec(peoples);
}
