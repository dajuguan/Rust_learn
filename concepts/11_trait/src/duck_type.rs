trait Human {
    fn speak(&self);
}

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

#[test]
fn test_duck_type() {
    let m = Man {};
    let w = WoMan {};
    let peoples: Vec<&dyn Human> = vec![&m, &w];
    for p in peoples {
        p.speak();
    }
}
