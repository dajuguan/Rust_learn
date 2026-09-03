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
use reth_trie_common::{BranchNodeCompact, HashedPostState, Nibbles};

use crate::sparse_trie::StateRootComputeOutcome;

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
    /// The current state root hash.
    state_root: B256,
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
                state_root: reth_trie::EMPTY_ROOT_HASH,
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

    /// Returns the current state root (cached from the last computation/commit).
    pub fn state_root(&self) -> B256 {
        let inner = self.inner.read().unwrap();
        inner.state_root
    }

    /// Applies the outcome from a sparse trie computation.
    ///
    /// This updates the hashed accounts and incrementally applies the trie node
    /// changes, avoiding a full recomputation.
    pub fn commit_outcome(&self, outcome: StateRootComputeOutcome) {
        let StateRootComputeOutcome { state_root, trie_updates, hashed_state } = outcome;
        let mut inner = self.inner.write().unwrap();

        // Apply account changes to hashed state
        for (addr, account) in &hashed_state.accounts {
            match account {
                Some(acc) => inner.hashed_accounts.insert(*addr, *acc),
                None => inner.hashed_accounts.remove(addr),
            };
        }

        // Apply trie node updates
        for (path, node) in trie_updates.account_nodes {
            inner.trie_nodes.insert(path, node);
        }
        for path in trie_updates.removed_nodes {
            inner.trie_nodes.remove(&path);
        }

        // Store the new state root
        inner.state_root = state_root;

        tracing::debug!(
            ?state_root,
            accounts = inner.hashed_accounts.len(),
            trie_nodes = inner.trie_nodes.len(),
            "inmem_db: committed outcome"
        );
    }

    // -- internal -----------------------------------------------------------

    /// Recomputes the trie from the current hashed accounts and stores the
    /// resulting branch nodes and state root.
    fn recompute_trie(&self) {
        let hashed_accounts = {
            let inner = self.inner.read().unwrap();
            inner.hashed_accounts.clone()
        };
        let (root, trie_nodes) = self.compute_root_and_nodes(&hashed_accounts);
        let mut inner = self.inner.write().unwrap();
        inner.trie_nodes = trie_nodes;
        inner.state_root = root;
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

/// Generates deterministic test accounts from a seed.
///
/// Returns a `BTreeMap` of hashed addresses to accounts, suitable for seeding
/// an [`InMemoryTrieDb`] or for independent root computation.
///
/// The seed ensures reproducibility across test runs.
pub fn generate_test_accounts(count: usize, seed: u64) -> BTreeMap<B256, Account> {
    use alloy_primitives::keccak256;

    let mut accounts = BTreeMap::new();
    for i in 0..count {
        // Derive a deterministic "address" from seed + index
        let mut addr_bytes = [0u8; 32];
        addr_bytes[0..8].copy_from_slice(&(seed.wrapping_add(i as u64)).to_le_bytes());
        addr_bytes[8..16].copy_from_slice(&(i as u64).to_le_bytes());
        let hashed_addr = keccak256(addr_bytes);

        // Deterministic nonce from seed
        let nonce = ((seed >> 3).wrapping_add(i as u64)) as u64;

        accounts.insert(hashed_addr, Account { nonce, balance: U256::ZERO, bytecode_hash: None });
    }
    accounts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_trie_nodes() {
        let db: InMemoryTrieDb = InMemoryTrieDb::new();

        // Generate accounts to create a deeper trie structure
        let initial_accounts = generate_test_accounts(29, 42);
        for (&addr, &account) in &initial_accounts {
            db.update_account(addr, Some(account));
        }

        let inner = db.inner.read().unwrap();

        // Collect all leaf paths (first 4 bytes = 8 hex chars)
        let mut leaf_paths: Vec<String> = inner
            .hashed_accounts
            .keys()
            .map(|addr| format!("{:x}", addr)[..8].to_string())
            .collect();
        leaf_paths.sort();

        // Collect all branch paths
        let mut branch_paths: Vec<String> = inner
            .trie_nodes
            .keys()
            .map(|n| n.iter().map(|b| format!("{:x}", b)).collect::<Vec<_>>().join(""))
            .collect();
        branch_paths.sort();

        // Print tree structure
        println!();
        println!("root (state_root: {:?})", inner.state_root);
        println!(" │");

        // Group leaves by first nibble to show logical trie structure
        let mut by_prefix: BTreeMap<String, Vec<&String>> = BTreeMap::new();
        for leaf in &leaf_paths {
            let prefix = &leaf[..1];
            by_prefix.entry(prefix.to_string()).or_default().push(leaf);
        }

        println!(" ├── LOGICAL TRIE STRUCTURE (all branches, including implicit):");
        for (prefix, leaves) in &by_prefix {
            let is_cached = branch_paths.contains(prefix);
            let cached_marker = if is_cached { "✓ cached" } else { "implicit" };

            if leaves.len() == 1 {
                // Single leaf - direct child of root (no branch needed)
                println!(" │   └─ [{}]: leaf {}", prefix, leaves[0]);
            } else {
                // Multiple leaves - there's a branch node here
                println!(
                    " │   ├─ [{}]: branch ({}) → {} leaves",
                    prefix,
                    cached_marker,
                    leaves.len()
                );
                for leaf in leaves {
                    println!(" │   │     └─ {}", leaf);
                }
            }
        }

        println!();
        println!(" ├── CACHED BRANCH NODES (stored in trie_nodes): {}", branch_paths.len());
        for (path, node) in &inner.trie_nodes {
            let path_str: String = path.iter().map(|b| format!("{:x}", b)).collect();
            println!(" │   [{}]:", path_str);
            println!(" │     state_mask: {:?} (children at positions)", node.state_mask);
            println!(" │     tree_mask:  {:?} (children with sub-tries)", node.tree_mask);
            println!(" │     hash_mask:  {:?} (children stored as hashes)", node.hash_mask);
            println!(" │     hashes: {} items", node.hashes.len());
            for (i, hash) in node.hashes.iter().enumerate() {
                println!(" │       [{}]: {:?}", i, hash);
            }
        }

        println!();
        println!(
            "Summary: {} accounts, {} cached branch nodes",
            inner.hashed_accounts.len(),
            inner.trie_nodes.len()
        );
        println!("Note: 'implicit' branches exist logically but aren't cached in trie_nodes.");
        println!(
            "      Only branches with sub-structure are stored for efficient proof generation."
        );
    }
}
