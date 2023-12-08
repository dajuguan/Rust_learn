// Ref: https://github.com/serde-rs/json/issues/456
use std::{collections::{BTreeMap, HashMap}, hash::Hasher};
use serde::{Serialize, Deserialize};



#[derive( Debug)]
pub struct Bar<K,V> {
    pub strings: BTreeMap<K, V>
}

#[derive(Serialize, Deserialize)]
struct Entry<K,V> {
    key: K,
    val: V
}

impl <K,V> Serialize for Bar<K,V>
where
    K: Eq + Serialize,
    V:Serialize {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer {

        serializer.collect_seq(self.strings.iter().map(|(key, val)| Entry { key, val }))
    }
}

impl<'de, K, V> Deserialize<'de> for Bar<K,V> 
where
    K: Ord + Deserialize<'de>,
    V: Deserialize<'de>
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de> {
        Vec::<Entry<K,V>>::deserialize(deserializer)
        .map(|mut v| 
            Bar{strings: v.drain(..).map(|kv| (kv.key, kv.val)).collect()
            })
    }
}

fn main() {
    let mut b = BTreeMap::new();
    // b.insert(31, 123);
    b.insert((31,32), 123);

    let bar = Bar {strings: b};

    // prints {"foo":123}
    let s = serde_json::to_string(&bar).unwrap();
    println!("{}", s);
    let deserialized: Bar<(i32,i32), i32> = serde_json::from_str(&s).unwrap();
    println!("deserialized:{:?}", deserialized);
}