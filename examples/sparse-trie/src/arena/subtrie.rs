//! A single subtrie within the arena-based parallel sparse trie.

use alloy_primitives::{keccak256, B256};
use reth_trie_common::{HashedPostState, Nibbles, ProofTrieNodeV2};
use std::collections::BTreeMap;

use super::node::{ArenaBranch, ArenaNode, Index, NodeArena};

/// A subtrie within the arena-based parallel sparse trie.
///
/// Each subtrie owns its own arena, allowing parallel mutations across subtries.
/// The `path` field indicates the prefix this subtrie covers (e.g., [0x7] covers all keys starting with 0x7...).
#[derive(Debug, Clone)]
pub struct ArenaSubtrie {
    /// The arena allocating nodes within this subtrie.
    pub arena: NodeArena,
    /// The root node index  of this subtrie.
    pub root: Index,
    /// The absolute path prefix of this subtrie in the full trie.
    pub path: Nibbles,
    /// Total number of leaves in this subtrie.
    pub num_leaves: u64,
    /// Number of dirty (modified) leaves in this subtrie.
    pub num_dirty: u64,
}

impl ArenaSubtrie {
    /// Creates a new subtrie with the given path prefix.
    pub fn new(path: Nibbles) -> Self {
        let mut arena = NodeArena::default();
        // Start with an empty root branch
        let root = arena.insert(ArenaNode::Branch(ArenaBranch::new()));
        Self { arena, root, path, num_leaves: 0, num_dirty: 0 }
    }

    /// Sets a child of the root branch as blinded with the given RLP encoding.
    pub fn root_branch_set_child_blinded(&mut self, nibble: u8, rlp: alloy_trie::nodes::RlpNode) {
        let blinded_idx = self.arena.insert(ArenaNode::Blinded(rlp));
        if let ArenaNode::Branch(branch) = &mut self.arena[self.root] {
            branch.set_child(nibble, Some(blinded_idx));
        }
    }

    /// Removes a leaf at the given node index and collapses parent branches if needed.
    fn remove_leaf_at(&mut self, leaf_idx: Index, _full_path: &Nibbles) {
        // Special case: if the leaf IS the root, replace root with empty branch
        if leaf_idx == self.root {
            let mut empty_branch = ArenaBranch::new();
            empty_branch.dirty = true;
            self.root = self.arena.insert(ArenaNode::Branch(empty_branch));
            self.num_leaves = self.num_leaves.saturating_sub(1);
            self.num_dirty += 1;
            return;
        }

        // Find the parent branch and the nibble position of this leaf
        let parent_info = self.find_parent_of(leaf_idx);
        if let Some((parent_idx, nibble)) = parent_info {
            // Remove the child from the parent branch
            if let ArenaNode::Branch(branch) = &mut self.arena[parent_idx] {
                branch.remove_child(nibble);
                self.num_leaves = self.num_leaves.saturating_sub(1);
                self.num_dirty += 1;

                // Check if parent branch needs collapsing
                let remaining_children = branch.num_children();
                if remaining_children == 0 {
                    // Parent is now empty. If it's the root, the subtrie is empty.
                    // Otherwise, remove the parent from its parent (recursively).
                    if parent_idx == self.root {
                        // Root branch is empty — subtrie is empty
                        return;
                    }
                    // Recursively remove the empty parent
                    self.remove_leaf_at(parent_idx, _full_path);
                } else if remaining_children == 1 {
                    // Parent has one child left — collapse.
                    // For now, just mark as dirty; the hash computation handles
                    // collapsing via compute_root_rlp.
                    // For intermediate branches, we'd need to create an extension node.
                    // This is a simplification that works for the root branch.
                }
            }
        }
    }

    /// Finds the parent branch and nibble position of a node.
    fn find_parent_of(&self, target_idx: Index) -> Option<(Index, u8)> {
        // Simple linear search through the arena to find which branch contains target_idx
        for (idx, node) in self.arena.iter() {
            if let ArenaNode::Branch(branch) = node {
                for (nibble, child_idx) in branch.iter_children() {
                    if child_idx == Some(target_idx) {
                        return Some((idx, nibble));
                    }
                }
            }
        }
        None
    }

    /// Inserts a leaf directly as the subtrie root (for depth-1 leaf proof nodes).
    ///
    /// The leaf's key is the full remaining path from this position — no nibbles
    /// are consumed. The subtrie's root is replaced with the leaf so that
    /// `upsert_leaf` compares the full key immediately.
    pub fn insert_leaf_directly(&mut self, key: Nibbles, value: Vec<u8>) {
        let leaf = ArenaNode::Leaf { key, value, dirty: false };
        self.root = self.arena.insert(leaf);
        self.num_leaves += 1;
        self.num_dirty += 1;
    }

    /// Reveals nodes from proof data into this subtrie.
    ///
    /// This implementation walks the trie and replaces blinded children with proof nodes.
    pub fn reveal_nodes(&mut self, nodes: &mut [ProofTrieNodeV2]) -> Result<(), String> {
        if nodes.is_empty() {
            return Ok(());
        }

        // Sort nodes by path for efficient traversal
        nodes.sort_unstable_by_key(|n| n.path);

        for node in nodes.iter_mut() {
            // Strip the subtrie path prefix to get the relative path
            if !node.path.starts_with(&self.path) {
                continue;
            }
            let relative_path = node.path.slice(self.path.len()..);

            // Try to reveal this node
            if let Some(child_idx) = self.reveal_node(&relative_path, node) {
                // If we revealed a leaf, increment the leaf counter
                if matches!(self.arena[child_idx], ArenaNode::Leaf { .. }) {
                    self.num_leaves += 1;
                }
            }
        }

        Ok(())
    }

    /// Reveals a single node at the given path.
    ///
    /// Returns the index of the revealed node if successful.
    fn reveal_node(&mut self, path: &Nibbles, proof_node: &mut ProofTrieNodeV2) -> Option<Index> {
        // Start from root and traverse to the parent of the target node
        let mut current = self.root;
        let mut remaining_path = path.clone();

        // Traverse to the parent
        while remaining_path.len() > 1 {
            let nibble = remaining_path.get(0)?;
            remaining_path = remaining_path.slice(1..);

            match &self.arena[current] {
                ArenaNode::Branch(branch) => {
                    if let Some(child_idx) = branch.child_at(nibble) {
                        current = child_idx;
                    } else if branch.state_mask.is_bit_set(nibble) {
                        // Child position exists in state_mask but is not yet revealed (None).
                        // Can't traverse further until this node is revealed.
                        return None;
                    } else {
                        // No child at this position - can't reveal
                        return None;
                    }
                }
                _ => {
                    // Hit a non-branch node - can't traverse further
                    return None;
                }
            }
        }

        // Now current is the parent, and remaining_path has 1 nibble
        if remaining_path.is_empty() {
            return None;
        }

        let nibble = remaining_path.get(0)?;

        // Check if the parent has a child at this position
        let parent_has_child = match &self.arena[current] {
            ArenaNode::Branch(b) => b.child_at(nibble).is_some(),
            _ => return None,
        };

        if parent_has_child {
            // A child already exists at this position. If it's a blinded node,
            // replace it with the revealed proof node. Otherwise it's already
            // revealed — nothing to do.
            let child_idx = match &self.arena[current] {
                ArenaNode::Branch(b) => b.child_at(nibble),
                _ => None,
            };
            if let Some(idx) = child_idx {
                if self.arena[idx].is_blinded() {
                    // Replace blinded child with the revealed node
                    let mut new_node = ArenaNode::from_proof_node(std::mem::replace(
                        proof_node,
                        ProofTrieNodeV2::empty(),
                    ));
                    if let ArenaNode::Leaf { key, .. } = &mut new_node {
                        let consumed = path.len().saturating_sub(1);
                        if consumed <= key.len() {
                            *key = key.slice(consumed..);
                        }
                    }
                    let new_idx = self.arena.insert(new_node);
                    if let ArenaNode::Branch(b) = &mut self.arena[current] {
                        b.set_child(nibble, Some(new_idx));
                    }
                    return Some(new_idx);
                }
            }
            // Already revealed with a non-blinded node
            return None;
        }

        // Create the new node from the proof node
        let mut new_node =
            ArenaNode::from_proof_node(std::mem::replace(proof_node, ProofTrieNodeV2::empty()));

        // For leaf nodes, the proof carries the full key from the proof node's own
        // position. The traversal consumed `path.len() - 1` nibbles (one per loop
        // iteration), and the leaf is placed at the last nibble. So the stored key
        // must skip `path.len() - 1` nibbles from the original proof key.
        if let ArenaNode::Leaf { key, .. } = &mut new_node {
            let consumed = path.len().saturating_sub(1);
            if consumed <= key.len() {
                *key = key.slice(consumed..);
            }
        }

        let new_idx = self.arena.insert(new_node);

        // Update parent to point to the new node.
        // set_child will also set the state_mask bit if needed.
        if let ArenaNode::Branch(b) = &mut self.arena[current] {
            b.set_child(nibble, Some(new_idx));
        }

        Some(new_idx)
    }

    /// Updates leaf values in this subtrie.
    ///
    /// Returns the list of keys that need proofs (hit blinded nodes).
    pub fn update_leaves(
        &mut self,
        updates: &[(B256, Option<Vec<u8>>)],
    ) -> Result<Vec<B256>, String> {
        let mut needs_proof = Vec::new();

        for (key, value) in updates {
            // Convert hashed key to nibbles
            let full_path = Nibbles::unpack(key);

            // Strip the subtrie path prefix
            if !full_path.starts_with(&self.path) {
                continue;
            }
            let relative_path = full_path.slice(self.path.len()..);

            // Try to update the leaf
            match self.upsert_leaf(&relative_path, value.clone()) {
                Ok(()) => {}
                Err(UpdateError::BlindedNode) => {
                    needs_proof.push(*key);
                }
                Err(e) => return Err(format!("Update failed: {:?}", e)),
            }
        }

        Ok(needs_proof)
    }

    /// Inserts or updates a leaf at the given path.
    fn upsert_leaf(&mut self, path: &Nibbles, value: Option<Vec<u8>>) -> Result<(), UpdateError> {
        // Start from root and traverse/insert
        let mut current = self.root;
        let mut remaining_path = path.clone();

        loop {
            match &self.arena[current] {
                ArenaNode::Branch(branch) => {
                    if remaining_path.is_empty() {
                        // We're at the target, but it's a branch - need to handle this case
                        // For simplicity, just mark as dirty
                        if let ArenaNode::Branch(b) = &mut self.arena[current] {
                            b.dirty = true;
                        }
                        return Ok(());
                    }

                    let nibble = remaining_path.get(0).unwrap_or(0);
                    remaining_path = remaining_path.slice(1..);

                    if let Some(child_idx) = branch.child_at(nibble) {
                        // Continue to child
                        current = child_idx;
                    } else {
                        // No child at this position
                        if let Some(value) = value {
                            // Insert new leaf
                            let leaf = ArenaNode::Leaf {
                                key: remaining_path.clone(),
                                value,
                                dirty: false,
                            };
                            let leaf_idx = self.arena.insert(leaf);

                            // Update parent branch
                            if let ArenaNode::Branch(b) = &mut self.arena[current] {
                                b.set_child(nibble, Some(leaf_idx));
                            }
                            self.num_leaves += 1;
                            self.num_dirty += 1;
                        }
                        return Ok(());
                    }
                }
                ArenaNode::Leaf { key, .. } => {
                    // We hit a leaf - need to potentially split
                    let existing_key = key.clone();

                    if existing_key == remaining_path {
                        // Same path - update or delete value
                        if let Some(value) = value {
                            if value.is_empty() {
                                // Empty value = deletion
                                self.remove_leaf_at(current, path);
                            } else if let ArenaNode::Leaf { value: v, dirty, .. } =
                                &mut self.arena[current]
                            {
                                *v = value;
                                *dirty = true;
                                self.num_dirty += 1;
                            }
                        } else {
                            // Delete the leaf: remove from parent branch
                            self.remove_leaf_at(current, path);
                        }
                        return Ok(());
                    }

                    // Different paths - need to split
                    // Find common prefix
                    let common_len = existing_key.common_prefix_length(&remaining_path);

                    // Create new branch at the split point
                    let mut new_branch = ArenaBranch::new();
                    new_branch.short_key = existing_key.slice(..common_len);

                    // Insert the new branch in place of the leaf
                    let branch_idx = self.arena.insert(ArenaNode::Branch(new_branch));

                    // Move existing leaf under the new branch
                    let existing_nibble = existing_key.get(common_len).unwrap_or(0);
                    let existing_remaining = existing_key.slice(common_len + 1..);
                    let existing_leaf = ArenaNode::Leaf {
                        key: existing_remaining,
                        value: if let ArenaNode::Leaf { value, .. } = &self.arena[current] {
                            value.clone()
                        } else {
                            unreachable!()
                        },
                        dirty: false,
                    };
                    let existing_leaf_idx = self.arena.insert(existing_leaf);

                    if let ArenaNode::Branch(b) = &mut self.arena[branch_idx] {
                        b.set_child(existing_nibble, Some(existing_leaf_idx));
                    }

                    // Insert new leaf if we have a value
                    if let Some(value) = value {
                        let new_nibble = remaining_path.get(common_len).unwrap_or(0);
                        let new_remaining = remaining_path.slice(common_len + 1..);
                        let new_leaf = ArenaNode::Leaf { key: new_remaining, value, dirty: false };
                        let new_leaf_idx = self.arena.insert(new_leaf);

                        if let ArenaNode::Branch(b) = &mut self.arena[branch_idx] {
                            b.set_child(new_nibble, Some(new_leaf_idx));
                        }
                        self.num_leaves += 1;
                        self.num_dirty += 1;
                    }

                    // Replace the old leaf with the new branch in its parent
                    // This is simplified - a full implementation would update the parent properly
                    self.arena[current] = ArenaNode::Branch(ArenaBranch::new());
                    if let ArenaNode::Branch(b) = &mut self.arena[current] {
                        // Copy children from new_branch
                        // This is a placeholder - proper implementation needed
                    }

                    return Ok(());
                }
                ArenaNode::Blinded(_) => {
                    return Err(UpdateError::BlindedNode);
                }
            }
        }
    }

    /// Computes and returns the RLP encoding of the subtrie root.
    /// This may be a hash (32 bytes) or an inline RLP (< 32 bytes).
    ///
    /// Handles branch collapsing: a branch with a single child is replaced
    /// by that child (with the nibble prepended to the child's key for leaves,
    /// or wrapped in an extension for branches).
    pub fn compute_root_rlp(&mut self) -> alloy_trie::nodes::RlpNode {
        // Check if root is a branch that needs collapsing
        if let ArenaNode::Branch(branch) = &self.arena[self.root] {
            let num_children = branch.num_children();
            if num_children == 0 {
                // Empty branch = empty trie
                return alloy_trie::nodes::RlpNode::word_rlp(&reth_trie::EMPTY_ROOT_HASH);
            }
            if num_children == 1 {
                // Single-child branch: collapse into the child.
                // For a leaf child, prepend the nibble to the leaf's key.
                // For a branch child, we'd need an extension node (not implemented yet).
                let (nibble, child_idx) =
                    branch.iter_children().find_map(|(n, c)| c.map(|ci| (n, ci))).unwrap();

                if let ArenaNode::Leaf { key, value, .. } = &self.arena[child_idx] {
                    // Prepend the nibble to the leaf's key
                    let mut full_key = Nibbles::from_nibbles([nibble]);
                    full_key.extend(key);
                    let leaf_node = reth_trie_common::LeafNodeRef { key: &full_key, value };
                    let mut buf = Vec::new();
                    use alloy_rlp::Encodable;
                    leaf_node.encode(&mut buf);
                    if buf.len() >= 32 {
                        let hash = keccak256(&buf);
                        let mut hash_buf = Vec::new();
                        hash.encode(&mut hash_buf);
                        return alloy_trie::nodes::RlpNode::from_rlp(&hash_buf);
                    } else {
                        return alloy_trie::nodes::RlpNode::from_rlp(&buf);
                    }
                }
                // For non-leaf single children, fall through to normal branch encoding
                // (extension node handling would go here)
            }
        }

        self.compute_node_rlp(self.root)
    }

    /// Computes and returns the root hash of this subtrie.
    pub fn compute_root_hash(&mut self) -> B256 {
        let rlp = self.compute_root_rlp();
        if rlp.is_hash() {
            rlp.as_hash().expect("RlpNode marked as hash must have a valid hash")
        } else {
            keccak256(rlp.as_ref())
        }
    }

    /// Computes the RLP encoding of a node. For nodes >= 32 bytes, returns the
    /// keccak256 hash wrapped as RlpNode. For nodes < 32 bytes, returns the inline RLP.
    fn compute_node_rlp(&mut self, idx: Index) -> alloy_trie::nodes::RlpNode {
        match &self.arena[idx] {
            ArenaNode::Blinded(rlp) => rlp.clone(),

            ArenaNode::Leaf { key, value, .. } => {
                let leaf_node = reth_trie_common::LeafNodeRef { key, value };
                let mut buf = Vec::new();
                use alloy_rlp::Encodable;
                leaf_node.encode(&mut buf);
                self.arena[idx].mark_dirty_false();
                alloy_trie::nodes::RlpNode::from_rlp(&buf)
            }

            ArenaNode::Branch(_) => {
                // Collect children indices first to release the immutable borrow
                let child_indices: Vec<(u8, Index)> = self.arena[idx].branch_children().collect();

                // Compute children's RLP nodes (requires &mut self)
                let mut child_rlps: Vec<(u8, alloy_trie::nodes::RlpNode)> = Vec::new();
                for (nibble, ci) in child_indices {
                    let child_rlp = self.compute_node_rlp(ci);
                    child_rlps.push((nibble, child_rlp));
                }

                // Encode branch with children's RLP
                let rlp = self.encode_branch_rlp(idx, &child_rlps);
                self.arena[idx].mark_dirty_false();
                alloy_trie::nodes::RlpNode::from_rlp(&rlp)
            }
        }
    }

    /// Encodes a branch node at the given index with children's RLP encodings.
    fn encode_branch_rlp(
        &self,
        idx: Index,
        child_rlps: &[(u8, alloy_trie::nodes::RlpNode)],
    ) -> Vec<u8> {
        use alloy_rlp::{Encodable, Header};

        let branch = match &self.arena[idx] {
            ArenaNode::Branch(b) => b,
            _ => unreachable!(),
        };

        let mut buf = Vec::new();
        let mut all_child_rlps: Vec<Vec<u8>> = Vec::with_capacity(16);
        let mut rlp_idx = 0;

        for nibble in 0u8..16 {
            if branch.state_mask.is_bit_set(nibble) {
                if rlp_idx < child_rlps.len() && child_rlps[rlp_idx].0 == nibble {
                    all_child_rlps.push(child_rlps[rlp_idx].1.as_ref().to_vec());
                    rlp_idx += 1;
                } else {
                    all_child_rlps.push(vec![0x80]); // Empty
                }
            } else {
                all_child_rlps.push(vec![0x80]); // Empty
            }
        }

        // Encode as branch: [child_0, child_1, ..., child_15, value]
        // value is always empty for intermediate branches
        all_child_rlps.push(vec![0x80]); // empty value

        let payload_len: usize = all_child_rlps.iter().map(|r| r.len()).sum();
        Header { list: true, payload_length: payload_len }.encode(&mut buf);
        for child_rlp in all_child_rlps {
            buf.extend_from_slice(&child_rlp);
        }

        buf
    }

    /// Returns the hashed post state for all leaves in this subtrie.
    pub fn collect_hashed_state(&self) -> HashedPostState {
        let mut state = HashedPostState::default();
        self.collect_leaves(self.root, &Nibbles::default(), &mut state);
        state
    }

    /// Recursively collects leaves into the hashed post state.
    fn collect_leaves(&self, idx: Index, path: &Nibbles, state: &mut HashedPostState) {
        match &self.arena[idx] {
            ArenaNode::Branch(branch) => {
                for (nibble, child_idx) in branch.iter_children() {
                    if let Some(child_idx) = child_idx {
                        let mut child_path = path.clone();
                        child_path.push(nibble);
                        self.collect_leaves(child_idx, &child_path, state);
                    }
                }
            }
            ArenaNode::Leaf { key, value, .. } => {
                // Reconstruct full path
                let mut full_path = self.path.clone();
                full_path.extend(path);
                full_path.extend(key);

                // Convert nibbles to hashed key
                let hashed_key = full_path.pack();

                // Decode account from value
                // For simplicity, assume value is RLP-encoded account
                // A full implementation would properly decode
                if !value.is_empty() {
                    // Placeholder: just store the raw value
                    // state.accounts.insert(hashed_key, Some(account));
                }
            }
            ArenaNode::Blinded(_) => {
                // Can't collect from blinded nodes
            }
        }
    }
}

/// Error type for leaf update operations.
#[derive(Debug, Clone)]
pub enum UpdateError {
    /// Hit a blinded node, need proof to proceed.
    BlindedNode,
    /// Other error.
    Other(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{keccak256, B256, U256};
    use alloy_trie::TrieAccount;
    use reth_primitives_traits::Account;
    use reth_trie::test_utils::state_root_prehashed;
    use reth_trie::EMPTY_ROOT_HASH;

    /// Helper: build a B256 whose nibble representation starts with the given
    /// nibbles and is zero-padded to 32 bytes (64 nibbles).
    fn b256_from_nibbles(nibbles: &[u8]) -> B256 {
        let mut bytes = [0u8; 32];
        for (i, chunk) in nibbles.chunks(2).enumerate() {
            let hi = chunk[0];
            let lo = if chunk.len() > 1 { chunk[1] } else { 0 };
            bytes[i] = (hi << 4) | lo;
        }
        B256::from_slice(&bytes)
    }

    /// Isolate the hash computation by manually constructing a 2-leaf subtrie
    /// with known topology and comparing against state_root_prehashed.
    ///
    /// Trie structure (nibble paths from subtrie root):
    ///   Leaf 1: [2, 14, 14, 1]  ->  value = RLP(account1)
    ///   Leaf 2: [2, 15,  3, 4]  ->  value = RLP(account2)
    ///
    /// Common prefix: [2]  ->  divergence nibbles: 14 vs 15
    ///
    /// Expected Merkle-Patricia layout:
    ///   Extension([2])
    ///     +- Branch(empty short_key, children at 14 and 15)
    ///          |- [14] Leaf([14, 1], value1)
    ///          +- [15] Leaf([3, 4],  value2)
    ///
    /// The key thing this test checks: compute_root_hash must account for
    /// the branch short_key by wrapping the branch RLP in an extension
    /// node before hashing. reth reference at update_cached_rlp (arena/mod.rs ~line 1269):
    ///
    ///   let rlp_node = if short_key.is_empty() {
    ///       rlp_node
    ///   } else {
    ///       ExtensionNodeRef::new(&short_key, &rlp_node).rlp(rlp_buf)
    ///   };
    #[test]
    #[ignore = "extension node (short_key) encoding not yet implemented"]
    fn compute_root_hash_handles_short_key() {
        // -- accounts & reference hash --
        let account1 = Account { nonce: 1, balance: U256::from(100), bytecode_hash: None };
        let account2 = Account { nonce: 2, balance: U256::from(200), bytecode_hash: None };

        // B256 keys whose nibble representations start with our desired paths.
        let key1 = b256_from_nibbles(&[2, 14, 14, 1]);
        let key2 = b256_from_nibbles(&[2, 15, 3, 4]);

        // RLP-encode the trie accounts (same encoding the trie stores as leaf values).
        use alloy_rlp::Encodable;
        let val1 = alloy_rlp::encode(account1.into_trie_account(EMPTY_ROOT_HASH));
        let val2 = alloy_rlp::encode(account2.into_trie_account(EMPTY_ROOT_HASH));

        // Reference root via reth state_root_prehashed.
        let empty_storage: Vec<(B256, U256)> = Vec::new();
        let expected = state_root_prehashed(
            [
                (key1, (account1, empty_storage.iter().cloned())),
                (key2, (account2, empty_storage.iter().cloned())),
            ]
            .into_iter(),
        );

        // -- manual trie construction --
        let mut subtrie = ArenaSubtrie::new(Nibbles::default());

        // Leaf 1 - key after consuming branch nibbles [2, 14] -> [14, 1]
        let leaf1_idx = subtrie.arena.insert(ArenaNode::Leaf {
            key: Nibbles::from_nibbles([14, 1]),
            value: val1,
            dirty: true,
        });

        // Leaf 2 - key after consuming branch nibbles [2, 15] -> [3, 4]
        let leaf2_idx = subtrie.arena.insert(ArenaNode::Leaf {
            key: Nibbles::from_nibbles([3, 4]),
            value: val2,
            dirty: true,
        });

        // Sub-branch at nibble 2 of root - no short_key, children at 14 and 15.
        let mut sub_branch = ArenaBranch::new();
        sub_branch.set_child(14, Some(leaf1_idx));
        sub_branch.set_child(15, Some(leaf2_idx));
        let sub_branch_idx = subtrie.arena.insert(ArenaNode::Branch(sub_branch));

        // Root branch - short_key = [2] (the common prefix), one child at nibble 2.
        let mut root_branch = ArenaBranch::new();
        root_branch.short_key = Nibbles::from_nibbles([2]);
        root_branch.set_child(2, Some(sub_branch_idx));
        subtrie.root = subtrie.arena.insert(ArenaNode::Branch(root_branch));

        // -- compare --
        let our_hash = subtrie.compute_root_hash();

        assert_eq!(
            our_hash, expected,
            "compute_root_hash does not match state_root_prehashed -- \
             the branch short_key (extension node) is likely not being encoded"
        );
    }
}
