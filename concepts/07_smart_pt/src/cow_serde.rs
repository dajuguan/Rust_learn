use serde::{Deserialize, Serialize};
use std::borrow::Cow;

#[derive(Debug, Serialize, Deserialize)]
struct User<'input> {
    #[serde(borrow)]
    name: Cow<'input, str>,
    age: u8,
}

#[test]
fn it_works() {
    let user = User {
        name: "<NAME>".into(),
        age: 30,
    };
    let json = serde_json::to_string(&user).unwrap();
    let user: User = serde_json::from_str(&json).unwrap();

    match user.name {
        Cow::Borrowed(name) => println!("Borrowed: {}", name),
        Cow::Owned(name) => println!("Owned: {}", name),
    }
}