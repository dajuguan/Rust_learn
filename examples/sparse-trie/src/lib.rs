mod db;
mod errors;
mod overlay;
mod proof_task;
mod sparse_trie;

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::overlay::OverlayManager;
    use alloy_primitives::{keccak256, map::B256Map, B256, U256};
    use reth_primitives_traits::Account;
    use reth_provider::DatabaseProviderROFactory;
    use reth_storage_errors::{db::DatabaseError, ProviderResult};
    use reth_trie::{
        hashed_cursor::{
            mock::{MockHashedCursor, MockHashedCursorFactory},
            HashedCursorFactory,
        },
        proof_v2::{self, SyncAccountValueEncoder},
        trie_cursor::{
            mock::{MockTrieCursor, MockTrieCursorFactory},
            TrieCursorFactory,
        },
    };
    use reth_trie_common::{BranchNodeCompact, HashedPostState, Nibbles};

    /// In-memory provider backed by mock cursor factories, so tests can seed real state.
    ///
    /// The trie side holds cached branch nodes and stays empty here, which makes the proof
    /// calculator walk and hash every leaf; the hashed side holds the canonical leaves.
    #[derive(Clone, Default)]
    struct InMemoryProvider {
        trie: MockTrieCursorFactory,
        hashed: MockHashedCursorFactory,
    }

    /// Gives every account an (possibly empty) storage entry so both factories agree.
    ///
    /// The mock storage cursors error on an unknown hashed address, and the account value encoder
    /// opens one per account leaf while computing that leaf's storage root.
    fn normalize_state(
        hashed_accounts: BTreeMap<B256, Account>,
        mut hashed_storages: B256Map<BTreeMap<B256, U256>>,
    ) -> (BTreeMap<B256, Account>, B256Map<BTreeMap<B256, U256>>) {
        for hashed_address in hashed_accounts.keys() {
            hashed_storages.entry(*hashed_address).or_default();
        }
        (hashed_accounts, hashed_storages)
    }

    /// No cached branch nodes anywhere, which forces the proof calculator to rehash every leaf.
    fn empty_storage_nodes(
        hashed_storages: &B256Map<BTreeMap<B256, U256>>,
    ) -> B256Map<BTreeMap<Nibbles, BranchNodeCompact>> {
        hashed_storages.keys().map(|hashed_address| (*hashed_address, BTreeMap::new())).collect()
    }

    impl InMemoryProvider {
        /// Provider holding the given hashed accounts and per-account storage leaves, with an empty
        /// intermediate trie.
        fn with_state(
            hashed_accounts: BTreeMap<B256, Account>,
            hashed_storages: B256Map<BTreeMap<B256, U256>>,
        ) -> Self {
            let (hashed_accounts, hashed_storages) =
                normalize_state(hashed_accounts, hashed_storages);
            Self {
                trie: MockTrieCursorFactory::new(
                    BTreeMap::new(),
                    empty_storage_nodes(&hashed_storages),
                ),
                hashed: MockHashedCursorFactory::new(hashed_accounts, hashed_storages),
            }
        }
    }

    impl TrieCursorFactory for InMemoryProvider {
        type AccountTrieCursor<'a>
            = MockTrieCursor
        where
            Self: 'a;
        type StorageTrieCursor<'a>
            = MockTrieCursor
        where
            Self: 'a;

        fn account_trie_cursor(&self) -> Result<Self::AccountTrieCursor<'_>, DatabaseError> {
            self.trie.account_trie_cursor()
        }

        fn storage_trie_cursor(
            &self,
            hashed_address: B256,
        ) -> Result<Self::StorageTrieCursor<'_>, DatabaseError> {
            self.trie.storage_trie_cursor(hashed_address)
        }
    }

    impl HashedCursorFactory for InMemoryProvider {
        type AccountCursor<'a>
            = MockHashedCursor<Account>
        where
            Self: 'a;
        type StorageCursor<'a>
            = MockHashedCursor<U256>
        where
            Self: 'a;

        fn hashed_account_cursor(&self) -> Result<Self::AccountCursor<'_>, DatabaseError> {
            self.hashed.hashed_account_cursor()
        }

        fn hashed_storage_cursor(
            &self,
            hashed_address: B256,
        ) -> Result<Self::StorageCursor<'_>, DatabaseError> {
            self.hashed.hashed_storage_cursor(hashed_address)
        }
    }

    /// Factory that satisfies `DatabaseProviderROFactory` for the in-memory provider.
    #[derive(Clone, Default)]
    struct InMemoryProviderFactory {
        provider: InMemoryProvider,
    }

    impl InMemoryProviderFactory {
        const fn new(provider: InMemoryProvider) -> Self {
            Self { provider }
        }
    }

    impl DatabaseProviderROFactory for InMemoryProviderFactory {
        type Provider = InMemoryProvider;

        fn database_provider_ro(&self) -> ProviderResult<Self::Provider> {
            Ok(self.provider.clone())
        }
    }

    /// Hashed address for a test account, mirroring how real state keys are derived.
    fn hashed_address(byte: u8) -> B256 {
        keccak256([byte])
    }

    /// State root recomputed straight from the seeded leaves, independent of any proof.
    fn state_root(provider: &InMemoryProvider) -> B256 {
        let mut value_encoder = SyncAccountValueEncoder::new(provider.clone(), provider.clone());
        let mut calculator = proof_v2::ProofCalculator::new(
            provider.account_trie_cursor().unwrap(),
            provider.hashed_account_cursor().unwrap(),
        );
        let root_node = calculator.root_node(&mut value_encoder).unwrap();
        calculator
            .compute_root_hash(core::slice::from_ref(&root_node))
            .unwrap()
            .expect("root node is at the empty path")
    }

    #[test]
    fn sparse_trie_task_works() {
        let trie_overlay = OverlayManager::default();
        let strategy = crate::sparse_trie::DefaultStateRootStrategy;
        let factory = InMemoryProviderFactory::default();
        let handle = strategy.spawn_sparse_trie_task(factory, &trie_overlay);

        // send some hashed state updates
        let state = reth_trie_common::HashedPostState::default();
        handle.on_hashed_state_update(state);

        // finish updates
        handle.on_updates_finished();
        // wait for the final state
        let final_root = handle.wait_for_final_root();
        match final_root {
            Some(root) => println!("Final state root: {:?}", root),
            None => {
                trie_overlay.clear_sparse_trie();
                eprintln!("Failed to retrieve final state root")
            }
        }
    }

    /// The task must reproduce the root of the state that results from the streamed updates, even
    /// though it only ever sees the accounts revealed by the proofs it requested.
    #[test]
    fn sparse_trie_task_matches_full_trie_root() {
        let hashed_accounts = BTreeMap::from([
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

        let factory = InMemoryProviderFactory::new(InMemoryProvider::with_state(
            hashed_accounts.clone(),
            B256Map::default(),
        ));
        let handle = crate::sparse_trie::DefaultStateRootStrategy
            .spawn_sparse_trie_task(factory, &OverlayManager::default());

        let changed = Account { nonce: 5, balance: U256::from(1), bytecode_hash: None };
        let created = Account { nonce: 0, balance: U256::from(7), bytecode_hash: None };

        // Streamed one account at a time, so the task has to interleave applying updates with
        // waiting for proofs instead of resolving everything in a single pass.
        let mut updates = vec![
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
        for update in updates.drain(..) {
            handle.on_hashed_state_update(update);
        }
        handle.on_updates_finished();

        let root = handle.wait_for_final_root().expect("state root");

        let mut expected_accounts = hashed_accounts;
        expected_accounts.remove(&hashed_address(1));
        expected_accounts.insert(hashed_address(2), changed);
        expected_accounts.insert(hashed_address(5), created);
        let expected_storages: B256Map<BTreeMap<B256, U256>> = B256Map::default();

        assert_eq!(
            root,
            state_root(&InMemoryProvider::with_state(expected_accounts, expected_storages))
        );
    }
}
