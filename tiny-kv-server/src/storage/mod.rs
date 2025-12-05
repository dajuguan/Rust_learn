mod memory;

pub use memory::MemStore;

use crate::KvError;

pub trait Storage<K, V> {
    fn get(&self, table: &K, key: &K) -> Result<Option<V>, KvError>;
    // set would be multithreading, so interior mutability is enough, that's why &self instead of &mut self is used.
    fn set(&self, table: &K, key: K, val: V) -> Result<Option<V>, KvError>;
}

#[cfg(test)]
mod tests {
    use crate::storage::memory::MemStore;

    use super::*;

    fn test_basic_trait(store: impl Storage<String, String>) {
        // set t1, k1, v1
        let t1 = "t1".to_string();
        let k1 = "k1".to_string();
        let v1 = "v1".to_string();
        let v0 = store.set(&t1, k1.clone(), v1.clone());
        assert!(v0.unwrap().is_none());

        // get t1, k1, v1
        let v1_get = store.get(&t1, &k1);
        assert_eq!(v1_get.unwrap(), Some(v1.clone()));

        // get none existed table or key
        let tnone = "tnone".to_string();
        let knone = "knone".to_string();
        assert!(store.get(&t1, &knone).unwrap().is_none());
        assert!(store.get(&tnone, &k1).unwrap().is_none());

        // set t1, k1, v2
        let v2 = "v2".to_string();
        let v1_pop = store.set(&t1, k1.clone(), v2.clone());
        assert_eq!(v1_pop.unwrap(), Some(v1.clone()));
        // get t1, k1, v2
        let v2_get = store.get(&t1, &k1);
        assert_eq!(v2_get.unwrap(), Some(v2.clone()));

        // set t2, k1, v1
        let t2 = "v2".to_string();
        let v1_pop = store.set(&t2, k1.clone(), v1.clone());
        assert!(v1_pop.unwrap().is_none());
        let v1_get = store.get(&t2, &k1);
        assert_eq!(v1_get.unwrap(), Some(v1.clone()));
    }

    #[test]
    fn memstore_basic_trait_should_work() {
        let store = MemStore::default();
        test_basic_trait(store);
    }
}
