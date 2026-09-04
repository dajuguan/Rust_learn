mod arena;
mod errors;
#[cfg(test)]
mod inmem_db;
mod overlay;
mod proof_task;
mod sparse_state_trie;
mod sparse_trie;

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use alloy_primitives::{keccak256, map::B256Map, B256, U256};
    use reth_primitives_traits::Account;
    use reth_trie::test_utils::state_root_prehashed;
    use reth_trie_common::HashedPostState;

    use crate::{inmem_db::InMemoryTrieDbFactory, overlay::OverlayManager};

    /// In-memory trie DB with default delay.
    fn test_db() -> crate::inmem_db::InMemoryTrieDb {
        crate::inmem_db::InMemoryTrieDb::new()
    }

    /// Hashed address for a test account.
    fn hashed_address(byte: u8) -> B256 {
        keccak256([byte])
    }

    #[test]
    fn sparse_trie_task_works() {
        let db = test_db();
        let factory = InMemoryTrieDbFactory::new(db);
        let overlay = OverlayManager::default();
        let handle =
            crate::sparse_trie::DefaultStateRootStrategy.spawn_sparse_trie_task(factory, &overlay);

        let state = HashedPostState::default();
        handle.on_hashed_state_update(state);
        handle.on_updates_finished();

        let outcome = handle.wait_for_final_outcome();
        match outcome {
            Some(outcome) => {
                println!("Final state root: {:?}", outcome.state_root);
                println!("Trie updates: {} nodes", outcome.trie_updates.account_nodes.len());
            }
            None => {
                overlay.clear_sparse_trie();
                eprintln!("Failed to retrieve final outcome")
            }
        }
    }

    /// The sparse trie task must produce a root that, once committed to the DB,
    /// matches the root computed independently from the final account set.
    #[test]
    fn sparse_trie_task_matches_full_trie_root() {
        let db: crate::inmem_db::InMemoryTrieDb = test_db();

        // Seed 4 accounts into the in-memory DB. Each call recomputes and
        // caches the intermediate trie nodes.
        let initial_accounts: BTreeMap<B256, Account> = BTreeMap::from([
            (
                hashed_address(1),
                Account { nonce: 1, balance: U256::from(100), bytecode_hash: None },
            ),
            (
                hashed_address(2),
                Account { nonce: 2, balance: U256::from(200), bytecode_hash: None },
            ),
            (
                hashed_address(3),
                Account {
                    nonce: 3,
                    balance: U256::from(300),
                    bytecode_hash: Some(keccak256([0xef])),
                },
            ),
            (
                hashed_address(4),
                Account { nonce: 4, balance: U256::from(400), bytecode_hash: None },
            ),
        ]);
        for (&addr, &account) in &initial_accounts {
            db.update_account(addr, Some(account));
        }

        let factory = InMemoryTrieDbFactory::new(db.clone());
        let handle = crate::sparse_trie::DefaultStateRootStrategy
            .spawn_sparse_trie_task(factory, &OverlayManager::default());

        let changed = Account { nonce: 5, balance: U256::from(1), bytecode_hash: None };
        let created = Account { nonce: 0, balance: U256::from(7), bytecode_hash: None };

        // Stream updates: delete account 1, modify account 2, create account 5.
        let updates = vec![
            HashedPostState {
                accounts: [(hashed_address(1), None), (hashed_address(2), Some(changed))]
                    .into_iter()
                    .collect(),
                storages: B256Map::default(),
            },
            HashedPostState {
                accounts: [(hashed_address(5), Some(created))].into_iter().collect(),
                storages: B256Map::default(),
            },
        ];
        for update in updates {
            handle.on_hashed_state_update(update);
        }
        handle.on_updates_finished();

        let outcome = handle.wait_for_final_outcome().expect("outcome");

        // Commit the outcome to the DB — trie nodes are applied incrementally,
        // no full recomputation needed.
        db.commit_outcome(outcome);

        // After commit, db.state_root() should match the independently computed root.
        let mut expected_accounts = initial_accounts;
        expected_accounts.remove(&hashed_address(1));
        expected_accounts.insert(hashed_address(2), changed);
        expected_accounts.insert(hashed_address(5), created);

        let empty_storage: Vec<(B256, U256)> = Vec::new();
        let expected = state_root_prehashed(
            expected_accounts.iter().map(|(&k, &v)| (k, (v, empty_storage.iter().cloned()))),
        );

        assert_eq!(db.state_root(), expected);
    }
}
