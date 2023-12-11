trait Pet {
    fn talk(&self);
}

struct Dog {
    name: String
}

impl Pet for Dog {
    fn talk(&self){
        println!("Dog says: {}", self.name);
    }
}

struct Cat {
    name: String
}

impl Pet for Cat {
    fn talk(&self){
        println!("Cat says: {}", self.name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let dog = Dog {
            name: "Wangwang".to_string()
        };
        let cat = Cat {
            name: "Miaomiao".to_string()
        };

        let pets: Vec<Box<dyn Pet>> = vec![Box::new(dog), Box::new(cat)];
        for pet in pets {
            pet.talk();
        }
    }
}