//! Sparse state trie wrapping ArenaParallelSparseTrie.
//!
//! This module provides a simplified version of reth's SparseStateTrie that
//! wraps our ArenaParallelSparseTrie implementation.

use alloy_primitives::B256;
use reth_trie_common::{updates::TrieUpdates, DecodedMultiProofV2};
use reth_trie_sparse::errors::{SparseTrieError, SparseTrieErrorKind};
use reth_trie_sparse::{RevealableSparseTrie, SparseTrie, TrieNodeEpoch};

use crate::arena::ArenaParallelSparseTrie;

/// Holds data that should be dropped after final state root is calculated.
#[derive(Debug, Default)]
pub struct DeferredDrops {
    /// Each nodes reveal operation creates a new buffer, uses it, and pushes it here.
    pub proof_nodes_bufs: Vec<Vec<reth_trie_common::ProofTrieNodeV2>>,
}

/// Sparse state trie representing lazy-loaded Ethereum state trie.
///
/// This is a simplified version of reth's SparseStateTrie that only handles
/// the account trie (no storage tries).
#[derive(Debug)]
pub struct SparseStateTrie {
    /// Sparse account trie.
    state: RevealableSparseTrie<ArenaParallelSparseTrie>,
    /// Flag indicating whether trie updates should be retained.
    retain_updates: bool,
    /// Holds data that should be dropped after final state root is calculated.
    deferred_drops: DeferredDrops,
}

impl Default for SparseStateTrie {
    fn default() -> Self {
        Self {
            state: Default::default(),
            retain_updates: false,
            deferred_drops: DeferredDrops::default(),
        }
    }
}

impl SparseStateTrie {
    /// Set the retention of branch node updates and deletions.
    pub const fn with_updates(mut self, retain_updates: bool) -> Self {
        self.retain_updates = retain_updates;
        self
    }

    /// Returns reference to bytes representing leaf value for the target account.
    pub fn get_account_value(&self, account: &B256) -> Option<&Vec<u8>> {
        self.state.as_revealed_ref()?.get_leaf_value(&reth_trie_common::Nibbles::unpack(account))
    }

    /// Returns mutable reference to the revealed account sparse trie.
    fn revealed_trie_mut(&mut self) -> Result<&mut ArenaParallelSparseTrie, SparseTrieError> {
        self.state.as_revealed_mut().ok_or_else(|| SparseTrieErrorKind::Blind.into())
    }

    /// Reveals a V2 decoded multiproof.
    pub fn reveal_decoded_multiproof_v2(
        &mut self,
        multiproof: DecodedMultiProofV2,
    ) -> Result<(), reth_trie_sparse::errors::SparseTrieError> {
        let DecodedMultiProofV2 { account_proofs, .. } = multiproof;

        if !account_proofs.is_empty() {
            let retain_updates = self.retain_updates;
            self.state.reveal_v2_proof_nodes(
                &mut account_proofs.into_iter().collect::<Vec<_>>(),
                retain_updates,
            )?;
        }

        Ok(())
    }

    /// Calculates the hashes of subtries.
    pub fn calculate_subtries(&mut self, new_epoch: TrieNodeEpoch) {
        if let RevealableSparseTrie::Revealed(trie) = &mut self.state {
            trie.update_subtrie_hashes(new_epoch);
        }
    }

    /// Returns sparse trie root and trie updates.
    pub fn root_with_updates(
        &mut self,
        new_epoch: TrieNodeEpoch,
    ) -> Result<(B256, TrieUpdates), reth_trie_sparse::errors::SparseTrieError> {
        let revealed = self.revealed_trie_mut()?;

        let root = revealed.root(new_epoch);
        let updates = revealed.take_updates();

        let trie_updates = TrieUpdates {
            account_nodes: updates.updated_nodes,
            removed_nodes: updates.removed_nodes,
            storage_tries: Default::default(), // We don't handle storage tries
        };

        Ok((root, trie_updates))
    }

    /// Returns mutable reference to the revealable sparse trie.
    /// This allows the caller to handle both blind and revealed states.
    pub fn trie_mut(&mut self) -> &mut RevealableSparseTrie<ArenaParallelSparseTrie> {
        &mut self.state
    }
}
