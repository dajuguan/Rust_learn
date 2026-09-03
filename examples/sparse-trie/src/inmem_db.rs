//! In-memory trie database for testing.
//!
//! Mimics a real DB by caching intermediate trie nodes and supporting account
//! mutations. Each [`update_account`](InMemoryTrieDb::update_account) call
//! recomputes the trie via [`StateRoot`] and stores the resulting branch nodes,
//! so subsequent proof reads hit cached intermediates instead of rebuilding
//! from leaves every time.
//!
//! A configurable delay is applied on every cursor-factory call to simulate DB
//! round-trip latency.

use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
    thread,
    time::Duration,
};

use alloy_primitives::{map::B256Map, B256, U256};
use reth_primitives_traits::Account;
use reth_provider::DatabaseProviderROFactory;
use reth_storage_errors::{db::DatabaseError, ProviderResult};
use reth_trie::{
    hashed_cursor::{
        mock::{MockHashedCursor, MockHashedCursorFactory},
        HashedCursorFactory,
    },
    trie_cursor::{
        mock::{MockTrieCursor, MockTrieCursorFactory},
        TrieCursorFactory,
    },
    StateRoot,
};
use reth_trie_common::{BranchNodeCompact, Nibbles};

/// Builds a storage map where every account has an (empty) storage entry.
///
/// `MockHashedCursorFactory` requires every account to have a storage trie
/// registered — even an empty one — otherwise `hashed_storage_cursor` errors
/// with "storage trie not found".
fn empty_storage_map(hashed_accounts: &BTreeMap<B256, Account>) -> B256Map<BTreeMap<B256, U256>> {
    hashed_accounts.keys().map(|&addr| (addr, BTreeMap::new())).collect()
}

/// Same as [`empty_storage_map`] but for trie-node storage tries (used by
/// `MockTrieCursorFactory`).
fn empty_storage_trie_map(
    hashed_accounts: &BTreeMap<B256, Account>,
) -> B256Map<BTreeMap<Nibbles, BranchNodeCompact>> {
    hashed_accounts.keys().map(|&addr| (addr, BTreeMap::new())).collect()
}

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct InMemoryTrieDbInner {
    hashed_accounts: BTreeMap<B256, Account>,
    /// Cached intermediate account-trie branch nodes, keyed by full nibble path.
    trie_nodes: BTreeMap<Nibbles, BranchNodeCompact>,
}

// ---------------------------------------------------------------------------
// InMemoryTrieDb
// ---------------------------------------------------------------------------

/// A mutable in-memory trie database.
///
/// Stores hashed accounts and cached intermediate trie nodes. Every account
/// mutation triggers a full trie recomputation via [`StateRoot`], mirroring
/// how a real DB persists `TrieUpdates` after each block.
///
/// Cloning is cheap (inner state is behind an `Arc`).
#[derive(Debug, Clone)]
pub struct InMemoryTrieDb {
    inner: Arc<RwLock<InMemoryTrieDbInner>>,
    delay: Duration,
}

impl InMemoryTrieDb {
    /// Creates an empty database with a 10µs default delay.
    pub fn new() -> Self {
        Self::with_delay(Duration::from_micros(10))
    }

    /// Creates an empty database with the given cursor-factory delay.
    ///
    /// `delay` is applied on every cursor-factory call (e.g.
    /// `account_trie_cursor()`, `hashed_account_cursor()`) to simulate DB
    /// round-trip latency.
    pub fn with_delay(delay: Duration) -> Self {
        Self {
            inner: Arc::new(RwLock::new(InMemoryTrieDbInner {
                hashed_accounts: BTreeMap::new(),
                trie_nodes: BTreeMap::new(),
            })),
            delay,
        }
    }

    /// Inserts, updates, or deletes an account and recomputes the trie.
    ///
    /// - `Some(account)` — insert or update.
    /// - `None` — delete.
    pub fn update_account(&self, hashed_addr: B256, account: Option<Account>) {
        {
            let mut inner = self.inner.write().unwrap();
            match account {
                Some(acc) => inner.hashed_accounts.insert(hashed_addr, acc),
                None => inner.hashed_accounts.remove(&hashed_addr),
            };
        }
        self.recompute_trie();
    }

    /// Returns the most recently computed state root.
    pub fn state_root(&self) -> B256 {
        let inner = self.inner.read().unwrap();
        self.compute_root_and_nodes(&inner.hashed_accounts).0
    }

    // -- internal -----------------------------------------------------------

    /// Recomputes the trie from the current hashed accounts and stores the
    /// resulting branch nodes.
    fn recompute_trie(&self) {
        let hashed_accounts = {
            let inner = self.inner.read().unwrap();
            inner.hashed_accounts.clone()
        };
        let (root, trie_nodes) = self.compute_root_and_nodes(&hashed_accounts);
        let mut inner = self.inner.write().unwrap();
        inner.trie_nodes = trie_nodes;
        tracing::debug!(?root, "inmem_db: trie recomputed");
    }

    /// Runs [`StateRoot`] over the given accounts (with an empty base trie)
    /// and returns `(root, account_nodes)`.
    fn compute_root_and_nodes(
        &self,
        hashed_accounts: &BTreeMap<B256, Account>,
    ) -> (B256, BTreeMap<Nibbles, BranchNodeCompact>) {
        let empty_trie =
            MockTrieCursorFactory::new(BTreeMap::new(), empty_storage_trie_map(hashed_accounts));
        let hashed = MockHashedCursorFactory::new(
            hashed_accounts.clone(),
            empty_storage_map(hashed_accounts),
        );

        let (root, updates) = StateRoot::new(empty_trie, hashed)
            .root_with_updates()
            .expect("state root computation should succeed");

        let nodes: BTreeMap<Nibbles, BranchNodeCompact> =
            updates.account_nodes.into_iter().collect();

        (root, nodes)
    }

    /// Snapshots the current state into a pair of mock factories.
    fn snapshot(&self) -> (MockTrieCursorFactory, MockHashedCursorFactory) {
        let inner = self.inner.read().unwrap();
        let trie = MockTrieCursorFactory::new(
            inner.trie_nodes.clone(),
            empty_storage_trie_map(&inner.hashed_accounts),
        );
        let hashed = MockHashedCursorFactory::new(
            inner.hashed_accounts.clone(),
            empty_storage_map(&inner.hashed_accounts),
        );
        (trie, hashed)
    }
}

// ---------------------------------------------------------------------------
// TrieCursorFactory — adds delay, then delegates to a snapshot-based mock
// ---------------------------------------------------------------------------

impl TrieCursorFactory for InMemoryTrieDb {
    type AccountTrieCursor<'a>
        = MockTrieCursor
    where
        Self: 'a;
    type StorageTrieCursor<'a>
        = MockTrieCursor
    where
        Self: 'a;

    fn account_trie_cursor(&self) -> Result<Self::AccountTrieCursor<'_>, DatabaseError> {
        thread::sleep(self.delay);
        let (trie, _) = self.snapshot();
        trie.account_trie_cursor()
    }

    fn storage_trie_cursor(
        &self,
        hashed_address: B256,
    ) -> Result<Self::StorageTrieCursor<'_>, DatabaseError> {
        thread::sleep(self.delay);
        let (trie, _) = self.snapshot();
        trie.storage_trie_cursor(hashed_address)
    }
}

// ---------------------------------------------------------------------------
// HashedCursorFactory — adds delay, then delegates to a snapshot-based mock
// ---------------------------------------------------------------------------

impl HashedCursorFactory for InMemoryTrieDb {
    type AccountCursor<'a>
        = MockHashedCursor<Account>
    where
        Self: 'a;
    type StorageCursor<'a>
        = MockHashedCursor<U256>
    where
        Self: 'a;

    fn hashed_account_cursor(&self) -> Result<Self::AccountCursor<'_>, DatabaseError> {
        thread::sleep(self.delay);
        let (_, hashed) = self.snapshot();
        hashed.hashed_account_cursor()
    }

    fn hashed_storage_cursor(
        &self,
        hashed_address: B256,
    ) -> Result<Self::StorageCursor<'_>, DatabaseError> {
        thread::sleep(self.delay);
        let (_, hashed) = self.snapshot();
        hashed.hashed_storage_cursor(hashed_address)
    }
}

// ---------------------------------------------------------------------------
// DatabaseProviderROFactory — the proof worker uses this to get a "provider"
// ---------------------------------------------------------------------------

/// Factory wrapper that satisfies [`DatabaseProviderROFactory`] for the proof
/// worker. The provider it returns is the [`InMemoryTrieDb`] itself (which
/// already implements both cursor-factory traits).
#[derive(Debug, Clone)]
pub struct InMemoryTrieDbFactory {
    db: InMemoryTrieDb,
}

impl InMemoryTrieDbFactory {
    pub const fn new(db: InMemoryTrieDb) -> Self {
        Self { db }
    }
}

impl DatabaseProviderROFactory for InMemoryTrieDbFactory {
    type Provider = InMemoryTrieDb;

    fn database_provider_ro(&self) -> ProviderResult<Self::Provider> {
        Ok(self.db.clone())
    }
}
