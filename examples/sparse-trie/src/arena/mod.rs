//! Arena-based parallel sparse trie implementation.
//!
//! This module provides a minimal implementation of a parallel sparse trie,
//! inspired by reth's `ArenaParallelSparseTrie`. The trie is split into two tiers:
//!
//! - **Upper trie** (depth 0-1): Stored in a fixed-size array, accessed serially.
//! - **Lower subtries** (depth >= 2): Each subtrie has its own arena, enabling lock-free parallel mutation.

mod node;
mod subtrie;

pub use node::{ArenaBranch, ArenaNode, Index, NodeArena};
pub use subtrie::{ArenaSubtrie, UpdateError};

use alloy_primitives::{map::B256Map, B256};
use alloy_trie::nodes::RlpNode;
use rayon::prelude::*;
use reth_trie_common::{
    BranchNodeMasks, Nibbles, ProofTrieNodeV2, ProofV2TargetParent, TrieNodeV2,
};
use reth_trie_sparse::errors::{SparseTrieError, SparseTrieErrorKind, SparseTrieResult};
use reth_trie_sparse::{LeafLookup, LeafLookupError, SparseTrie, SparseTrieUpdates, TrieNodeEpoch};
use std::borrow::Cow;

/// The maximum depth (in nibbles) for nodes in the upper trie.
/// Nodes at this depth or deeper belong to lower subtries.
const UPPER_TRIE_MAX_DEPTH: usize = 2;

/// A slot in the upper trie.
#[derive(Debug, Clone)]
enum UpperSlot {
    /// Empty slot.
    Empty,
    /// Blinded node - only hash is known.
    Blinded(RlpNode),
    /// Index into the subtries array.
    SubtrieIdx(u8),
}

/// An arena-based sparse trie whose subtries can be mutated in parallel.
///
/// The trie is split into two tiers:
/// - Upper trie (depth < 2): Fixed-size array, serial access
/// - Lower subtries (depth >= 2): Independent arenas, parallel access
#[derive(Debug)]
pub struct ArenaParallelSparseTrie {
    /// Upper trie slots (root's 16 children).
    upper: [UpperSlot; 16],
    /// Lower subtries, one per first nibble.
    subtries: [Option<Box<ArenaSubtrie>>; 16],
    /// Whether to track updates.
    retain_updates: bool,
    /// Accumulated updates.
    updates: SparseTrieUpdates,
}

impl Default for ArenaParallelSparseTrie {
    fn default() -> Self {
        Self {
            upper: std::array::from_fn(|_| UpperSlot::Empty),
            subtries: std::array::from_fn(|_| None),
            retain_updates: false,
            updates: SparseTrieUpdates::default(),
        }
    }
}

impl ArenaParallelSparseTrie {
    /// Creates a new empty arena parallel sparse trie.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if the given path should be stored in a subtrie.
    const fn should_be_subtrie(path_len: usize) -> bool {
        path_len >= UPPER_TRIE_MAX_DEPTH
    }

    /// Gets or creates a subtrie for the given first nibble.
    fn get_or_create_subtrie(&mut self, nibble: u8) -> &mut ArenaSubtrie {
        let idx = nibble as usize;
        if self.subtries[idx].is_none() {
            let mut path = Nibbles::default();
            path.push(nibble);
            self.subtries[idx] = Some(Box::new(ArenaSubtrie::new(path)));
            self.upper[idx] = UpperSlot::SubtrieIdx(nibble);
        }
        self.subtries[idx].as_deref_mut().unwrap()
    }

    /// Parallel reveal nodes across subtries.
    ///
    /// This implements parallel processing similar to reth's ArenaParallelSparseTrie:
    /// 1. Group nodes by first nibble
    /// 2. For subtries with enough nodes, take them for parallel processing
    /// 3. Use rayon to process subtries in parallel
    /// 4. Restore subtries after processing
    fn parallel_reveal(&mut self, nodes: &mut [ProofTrieNodeV2]) -> Result<(), String> {
        if nodes.is_empty() {
            return Ok(());
        }

        // Sort nodes by path for efficient grouping
        nodes.sort_unstable_by_key(|n| n.path);

        // Threshold for parallel processing (like reth's min_revealed_nodes)
        const PARALLEL_THRESHOLD: usize = 16;

        // Separate root node (path is empty) from the rest
        let mut depth_2_plus: Vec<ProofTrieNodeV2> = Vec::new();

        for node in nodes.iter() {
            if node.path.is_empty() {
                // Root node: initialize upper trie structure from the root branch.
                self.reveal_root_node(node)?;
            } else if node.path.len() == 1 {
                // Depth-1 node: create subtrie or mark upper slot as blinded.
                let nibble = node.path.get(0).unwrap_or(0) as usize;
                self.reveal_depth1_node(nibble, node)?;
            } else {
                depth_2_plus.push(node.clone());
            }
        }

        // Group depth-2+ nodes by first nibble and route to subtries
        let mut taken: Vec<(usize, Box<ArenaSubtrie>, Vec<ProofTrieNodeV2>)> = Vec::new();

        // Group by first nibble
        let mut grouped: [Vec<ProofTrieNodeV2>; 16] = std::array::from_fn(|_| Vec::new());
        for node in depth_2_plus {
            let nibble = node.path.get(0).unwrap_or(0) as usize;
            grouped[nibble].push(node);
        }

        for (nibble, group) in grouped.into_iter().enumerate() {
            if group.is_empty() {
                continue;
            }

            // Ensure subtrie exists for this nibble
            if self.subtries[nibble].is_none() {
                let mut path = Nibbles::default();
                path.push(nibble as u8);
                self.subtries[nibble] = Some(Box::new(ArenaSubtrie::new(path)));
                self.upper[nibble] = UpperSlot::SubtrieIdx(nibble as u8);
            }

            let num_nodes = group.len();
            if num_nodes >= PARALLEL_THRESHOLD {
                let subtrie = self.subtries[nibble].take().unwrap();
                taken.push((nibble, subtrie, group));
            } else {
                let mut group = group;
                self.subtries[nibble].as_deref_mut().unwrap().reveal_nodes(&mut group)?;
            }
        }

        // Process taken subtries in parallel
        if taken.len() == 1 {
            let (_, ref mut subtrie, ref mut node_vec) = taken[0];
            subtrie.reveal_nodes(node_vec)?;
        } else if taken.len() > 1 {
            use rayon::prelude::*;

            let results: Vec<Result<(), String>> = taken
                .par_iter_mut()
                .map(|(_, subtrie, node_vec)| subtrie.reveal_nodes(node_vec))
                .collect();

            for result in results {
                result?;
            }
        }

        // Restore taken subtries
        for (nibble, subtrie, _) in taken {
            self.subtries[nibble] = Some(subtrie);
        }

        Ok(())
    }

    /// Reveals the root node (path is empty) and initializes the upper trie structure.
    fn reveal_root_node(&mut self, node: &ProofTrieNodeV2) -> Result<(), String> {
        match &node.node {
            TrieNodeV2::EmptyRoot => {
                // Empty trie - nothing to do
            }
            TrieNodeV2::Branch(branch) => {
                // Initialize upper slots based on the root branch's state mask.
                // Each child in the branch stack corresponds to a nibble position.
                // Children that have proof nodes at depth 1 will be upgraded to SubtrieIdx
                // when those depth-1 nodes are processed.
                // For now, mark children present in the state_mask as Blinded.
                let mut child_idx = 0;
                for nibble in 0u8..16 {
                    if branch.state_mask.is_bit_set(nibble) {
                        // This child exists in the root branch.
                        // If it's already a SubtrieIdx (from a previous reveal), keep it.
                        // Otherwise, mark as Blinded (will be upgraded when depth-1 node arrives).
                        if !matches!(self.upper[nibble as usize], UpperSlot::SubtrieIdx(_)) {
                            // Extract the RLP encoding of this child from the branch stack
                            if let Some(stack_entry) = branch.stack.get(child_idx) {
                                self.upper[nibble as usize] =
                                    UpperSlot::Blinded(stack_entry.clone());
                            }
                        }
                        child_idx += 1;
                    }
                }
            }
            TrieNodeV2::Leaf(leaf) => {
                // Root is a leaf - the entire trie is a single leaf.
                // This is unusual but possible for a trie with one element.
                // Store as a blinded node for now.
                tracing::trace!("Root is a leaf node");
            }
            _ => {}
        }
        Ok(())
    }

    /// Reveals a depth-1 node and creates/updates the corresponding subtrie.
    fn reveal_depth1_node(&mut self, nibble: usize, node: &ProofTrieNodeV2) -> Result<(), String> {
        match &node.node {
            TrieNodeV2::Branch(branch) => {
                // Create subtrie for this nibble if it doesn't exist
                if self.subtries[nibble].is_none() {
                    let mut path = Nibbles::default();
                    path.push(nibble as u8);
                    self.subtries[nibble] = Some(Box::new(ArenaSubtrie::new(path)));
                }
                self.upper[nibble] = UpperSlot::SubtrieIdx(nibble as u8);

                // Initialize the subtrie's root branch with the proof node's children
                let subtrie = self.subtries[nibble].as_deref_mut().unwrap();
                let mut child_idx = 0;
                for child_nibble in 0u8..16 {
                    if branch.state_mask.is_bit_set(child_nibble) {
                        if let Some(stack_entry) = branch.stack.get(child_idx) {
                            // Mark the child as blinded in the subtrie's root branch
                            subtrie
                                .root_branch_set_child_blinded(child_nibble, stack_entry.clone());
                        }
                        child_idx += 1;
                    }
                }
            }
            TrieNodeV2::Leaf(leaf) => {
                // Depth-1 leaf: the leaf sits directly at the subtrie root position.
                // Its key is the full remaining path from this position — do NOT strip
                // any nibbles. Store it as a child of the subtrie root branch at the
                // nibble that matches the first nibble of the leaf's key.
                if self.subtries[nibble].is_none() {
                    let mut path = Nibbles::default();
                    path.push(nibble as u8);
                    self.subtries[nibble] = Some(Box::new(ArenaSubtrie::new(path)));
                }
                self.upper[nibble] = UpperSlot::SubtrieIdx(nibble as u8);

                let subtrie = self.subtries[nibble].as_deref_mut().unwrap();
                subtrie.insert_leaf_directly(leaf.key.clone(), leaf.value.clone());
            }
            TrieNodeV2::EmptyRoot => {
                // Empty child at depth 1
                self.upper[nibble] = UpperSlot::Empty;
            }
            _ => {}
        }
        Ok(())
    }

    /// Parallel leaf updates across subtries.
    fn parallel_update_leaves(
        &mut self,
        updates: &mut B256Map<reth_trie_sparse::LeafUpdate>,
        mut proof_required_fn: impl FnMut(B256, ProofV2TargetParent),
    ) -> Result<(), String> {
        // Group updates by first nibble
        let mut grouped: [Vec<(B256, Option<Vec<u8>>)>; 16] = std::array::from_fn(|_| Vec::new());

        for (key, update) in updates.iter() {
            let nibble = key.0[0] >> 4;
            let value = match update {
                reth_trie_sparse::LeafUpdate::Changed(v) => Some(v.clone()),
                reth_trie_sparse::LeafUpdate::Touched => None,
            };
            grouped[nibble as usize].push((*key, value));
        }

        // Process each subtrie that needs updating
        let mut all_keys_needing_proof: Vec<B256> = Vec::new();
        let mut applied_keys: Vec<B256> = Vec::new();
        for (nibble, group) in grouped.into_iter().enumerate() {
            if !group.is_empty() {
                if let Some(subtrie) = &mut self.subtries[nibble] {
                    let needs_proof = subtrie.update_leaves(&group)?;
                    for key in needs_proof {
                        // Request full proof from root (NONE) to ensure the proof
                        // covers the entire path to the target.
                        proof_required_fn(key, ProofV2TargetParent::NONE);
                        all_keys_needing_proof.push(key);
                    }
                } else if matches!(self.upper[nibble], UpperSlot::Empty) {
                    // No subtrie AND the root branch has no child at this nibble.
                    // The account definitely doesn't exist in the trie.
                    for (key, value) in &group {
                        match value {
                            None => {
                                // Deletion of non-existent account — no-op, consider it applied.
                                applied_keys.push(*key);
                            }
                            Some(val) => {
                                // Creation at a previously empty position.
                                // Create a subtrie and insert the leaf.
                                let mut path = Nibbles::default();
                                path.push(nibble as u8);
                                let mut subtrie = ArenaSubtrie::new(path);
                                // The target's full path (from the subtrie root) is the
                                // hashed address with the first nibble stripped (since the
                                // subtrie covers that nibble).
                                let full_path = Nibbles::unpack(key);
                                let relative_path = full_path.slice(1..);
                                subtrie.insert_leaf_directly(relative_path, val.clone());
                                self.subtries[nibble] = Some(Box::new(subtrie));
                                self.upper[nibble] = UpperSlot::SubtrieIdx(nibble as u8);
                                applied_keys.push(*key);
                            }
                        }
                    }
                } else {
                    // No subtrie exists but the root branch indicates a child exists
                    // (Blinded). All keys in this group need proofs to reveal the path.
                    for (key, _) in &group {
                        proof_required_fn(*key, ProofV2TargetParent::NONE);
                        all_keys_needing_proof.push(*key);
                    }
                }
            }
        }

        // Remove successfully updated keys; keep those that still need proofs.
        let needing_proof_set: std::collections::HashSet<B256> =
            all_keys_needing_proof.into_iter().collect();
        updates.retain(|key, _| needing_proof_set.contains(key));

        Ok(())
    }

    /// Computes the root hash by combining upper and lower trie hashes.
    fn compute_root_hash(&mut self) -> B256 {
        use alloy_primitives::keccak256;
        use alloy_rlp::{Encodable, Header};

        // Compute subtrie RLP nodes (may be inline or hash)
        let mut subtrie_rlps: [Option<alloy_trie::nodes::RlpNode>; 16] =
            std::array::from_fn(|_| None);

        for (nibble, subtrie) in self.subtries.iter_mut().enumerate() {
            if let Some(subtrie) = subtrie {
                subtrie_rlps[nibble] = Some(subtrie.compute_root_rlp());
            }
        }

        // Build root branch with subtrie RLPs
        let mut child_rlps: Vec<Vec<u8>> = Vec::with_capacity(17);
        for nibble in 0u8..16 {
            match &self.upper[nibble as usize] {
                UpperSlot::Empty => {
                    child_rlps.push(vec![0x80]); // Empty string
                }
                UpperSlot::Blinded(rlp) => {
                    child_rlps.push(rlp.as_ref().to_vec());
                }
                UpperSlot::SubtrieIdx(_) => {
                    if let Some(subtrie) = &self.subtries[nibble as usize] {
                        // Check if subtrie is empty (root branch has no children)
                        let is_empty = match &subtrie.arena[subtrie.root] {
                            ArenaNode::Branch(b) => b.num_children() == 0,
                            _ => false,
                        };
                        if is_empty {
                            child_rlps.push(vec![0x80]);
                        } else if let Some(rlp) = &subtrie_rlps[nibble as usize] {
                            child_rlps.push(rlp.as_ref().to_vec());
                        } else {
                            child_rlps.push(vec![0x80]);
                        }
                    } else {
                        child_rlps.push(vec![0x80]);
                    }
                }
            }
        }
        // 17th element: value (always empty for the root branch)
        child_rlps.push(vec![0x80]);

        // Encode as root branch
        let payload_len: usize = child_rlps.iter().map(|r| r.len()).sum();
        let mut buf = Vec::new();
        Header { list: true, payload_length: payload_len }.encode(&mut buf);
        for child_rlp in child_rlps {
            buf.extend_from_slice(&child_rlp);
        }

        keccak256(&buf)
    }
}

impl SparseTrie for ArenaParallelSparseTrie {
    fn set_root(
        &mut self,
        root: TrieNodeV2,
        masks: Option<BranchNodeMasks>,
        retain_updates: bool,
    ) -> SparseTrieResult<()> {
        self.retain_updates = retain_updates;
        // TODO: Initialize root node
        tracing::trace!("Setting root node");
        Ok(())
    }

    fn set_updates(&mut self, retain_updates: bool) {
        self.retain_updates = retain_updates;
    }

    fn reveal_nodes(&mut self, nodes: &mut [ProofTrieNodeV2]) -> SparseTrieResult<()> {
        self.parallel_reveal(nodes).map_err(|e| SparseTrieErrorKind::Other(e.into()).into())
    }

    fn root(&mut self, new_epoch: TrieNodeEpoch) -> B256 {
        self.compute_root_hash()
    }

    fn is_root_cached(&self) -> bool {
        false
    }

    fn root_epoch(&self) -> Option<TrieNodeEpoch> {
        None
    }

    fn update_subtrie_hashes(&mut self, new_epoch: TrieNodeEpoch) {
        // TODO: Implement
    }

    fn get_leaf_value(&self, full_path: &Nibbles) -> Option<&Vec<u8>> {
        // TODO: Implement
        None
    }

    fn find_leaf(
        &self,
        full_path: &Nibbles,
        expected_value: Option<&Vec<u8>>,
    ) -> Result<LeafLookup, LeafLookupError> {
        // TODO: Implement
        Err(LeafLookupError::BlindedNode { path: full_path.clone(), hash: B256::ZERO })
    }

    fn updates_ref(&self) -> Cow<'_, SparseTrieUpdates> {
        Cow::Borrowed(&self.updates)
    }

    fn take_updates(&mut self) -> SparseTrieUpdates {
        std::mem::take(&mut self.updates)
    }

    fn wipe(&mut self) {
        *self = Self::default();
    }

    fn clear(&mut self) {
        *self = Self::default();
    }

    fn prune(&mut self, prune_before: TrieNodeEpoch) -> usize {
        // TODO: Implement
        0
    }

    fn update_leaves(
        &mut self,
        updates: &mut B256Map<reth_trie_sparse::LeafUpdate>,
        proof_required_fn: impl FnMut(B256, ProofV2TargetParent),
    ) -> SparseTrieResult<()> {
        self.parallel_update_leaves(updates, proof_required_fn)
            .map_err(|e| SparseTrieErrorKind::Other(e.into()).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arena_parallel_sparse_trie_default() {
        let trie = ArenaParallelSparseTrie::new();
        assert_eq!(trie.subtries.iter().filter(|s| s.is_some()).count(), 0);
    }

    #[test]
    fn test_should_be_subtrie() {
        assert!(!ArenaParallelSparseTrie::should_be_subtrie(0));
        assert!(!ArenaParallelSparseTrie::should_be_subtrie(1));
        assert!(ArenaParallelSparseTrie::should_be_subtrie(2));
        assert!(ArenaParallelSparseTrie::should_be_subtrie(3));
    }
}
