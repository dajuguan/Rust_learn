use crate::{KvError, Storage};
use dashmap::{DashMap, mapref::one::Ref};
use std::hash::Hash;

#[derive(Debug, Clone, Default)]
pub struct MemStore<K: Eq + Hash, V> {
    tables: DashMap<K, DashMap<K, V>>,
}

impl<K, V> MemStore<K, V>
where
    K: Eq + Hash + Clone,
{
    fn get_or_create_table(&self, table: &K) -> Ref<K, DashMap<K, V>> {
        self.tables.entry(table.clone()).or_default().downgrade()
    }
}

impl<K, V> Storage<K, V> for MemStore<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    fn get(&self, table: &K, key: &K) -> Result<Option<V>, KvError> {
        let table = self.get_or_create_table(table);
        Ok(table.get(key).map(|v| v.value().clone()))
    }

    fn set(&self, table: &K, key: K, val: V) -> Result<Option<V>, KvError> {
        let table = self.get_or_create_table(table);
        Ok(table.insert(key, val))
    }
}
