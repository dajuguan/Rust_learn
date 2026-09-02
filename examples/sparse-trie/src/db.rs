//! In-memory state database: `HashedAccounts` is the only canonical latest-state
//! table (keyed by `keccak256(address)`), plus the proof source for the scheduler.
//!
//! Proof generation follows the reuse strategy from the README: no self-made MPT
//! code, everything goes through `alloy-trie`'s `HashBuilder` + `ProofRetainer`.

use std::collections::{BTreeMap, BTreeSet};

use alloy_primitives::{keccak256, Address, B256};
use alloy_rlp::Decodable;
use alloy_trie::{
    nodes::TrieNode, proof::ProofRetainer, HashBuilder, Nibbles, TrieAccount, EMPTY_ROOT_HASH,
};
use reth_trie_common::ProofTrieNodeV2;

/// Per-block hashed diff: `keccak256(address) -> Some(new account) | None (delete)`.
/// Sorted by hashed key so application order is deterministic.
// pub type HashedPostState = BTreeMap<B256, Option<TrieAccount>>;

#[derive(Debug, Default)]
pub struct StateDb {
    /// `HashedAddress -> Account`, ordered — the canonical latest state.
    pub trie: BTreeMap<Nibbles, TrieNode>,
    /// Current head block number.
    pub head: u64,
    /// State root of `head`, must always equal `full_root(hashed_accounts)`.
    pub state_root: B256,
}

impl StateDb {}
