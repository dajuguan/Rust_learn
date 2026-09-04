//! Node types for the arena-based sparse trie.

use alloy_primitives::B256;
use alloy_rlp::Encodable;
use alloy_trie::{nodes::RlpNode, TrieMask};
use reth_trie_common::{Nibbles, ProofTrieNodeV2, TrieNodeV2};
use slotmap::{DefaultKey, SlotMap};
use smallvec::SmallVec;

/// Index type for nodes in the arena.
pub type Index = DefaultKey;

/// The arena storing trie nodes.
pub type NodeArena = SlotMap<Index, ArenaNode>;

/// A node in the arena-based sparse trie.
#[derive(Debug, Clone)]
pub enum ArenaNode {
    /// A branch node with up to 16 children.
    Branch(ArenaBranch),
    /// A leaf node containing a value.
    Leaf {
        /// The remaining key suffix for this leaf.
        key: Nibbles,
        /// The RLP-encoded leaf value.
        value: Vec<u8>,
        /// Whether this node needs hash recomputation.
        dirty: bool,
    },
    /// A blinded node - only its RLP encoding (hash) is known.
    Blinded(RlpNode),
}

impl ArenaNode {
    /// Returns true if this node is a branch.
    pub const fn is_branch(&self) -> bool {
        matches!(self, ArenaNode::Branch(_))
    }

    /// Returns true if this node is a leaf.
    pub const fn is_leaf(&self) -> bool {
        matches!(self, ArenaNode::Leaf { .. })
    }

    /// Returns true if this node is blinded.
    pub const fn is_blinded(&self) -> bool {
        matches!(self, ArenaNode::Blinded(_))
    }

    /// Returns true if this node is dirty (needs hash recomputation).
    pub fn is_dirty(&self) -> bool {
        match self {
            ArenaNode::Branch(b) => b.dirty,
            ArenaNode::Leaf { dirty, .. } => *dirty,
            ArenaNode::Blinded(_) => false,
        }
    }

    /// Marks this node as dirty.
    pub fn mark_dirty(&mut self) {
        match self {
            ArenaNode::Branch(b) => b.dirty = true,
            ArenaNode::Leaf { dirty, .. } => *dirty = true,
            ArenaNode::Blinded(_) => {}
        }
    }

    /// Returns the short key (extension key) for branches, or the leaf key.
    pub fn short_key(&self) -> Option<&Nibbles> {
        match self {
            ArenaNode::Branch(b) => Some(&b.short_key),
            ArenaNode::Leaf { key, .. } => Some(key),
            ArenaNode::Blinded(_) => None,
        }
    }

    /// Sets dirty to false. No-op for blinded nodes.
    pub fn mark_dirty_false(&mut self) {
        match self {
            ArenaNode::Branch(b) => b.dirty = false,
            ArenaNode::Leaf { dirty, .. } => *dirty = false,
            ArenaNode::Blinded(_) => {}
        }
    }

    /// Returns an iterator over (nibble, child_index) pairs for branch children.
    /// Panics if called on a non-branch node.
    pub fn branch_children(&self) -> impl Iterator<Item = (u8, Index)> + '_ {
        match self {
            ArenaNode::Branch(b) => {
                b.iter_children().filter_map(|(nibble, c)| c.map(|ci| (nibble, ci)))
            }
            _ => panic!("branch_children called on non-branch node"),
        }
    }

    /// Computes and returns the RLP encoding of this node.
    pub fn compute_rlp(&self, arena: &NodeArena) -> Vec<u8> {
        let mut buf = Vec::new();
        match self {
            ArenaNode::Branch(branch) => {
                branch.encode_rlp(arena, &mut buf);
            }
            ArenaNode::Leaf { key, value, .. } => {
                // Leaf node: [key, value]
                let leaf_node = reth_trie_common::LeafNodeRef { key, value };
                leaf_node.encode(&mut buf);
            }
            ArenaNode::Blinded(rlp) => {
                // Already encoded
                buf.extend_from_slice(rlp.as_ref());
            }
        }
        buf
    }

    /// Computes and returns the hash of this node.
    pub fn compute_hash(&self, arena: &NodeArena) -> B256 {
        let rlp = self.compute_rlp(arena);
        if rlp.len() < 32 {
            // Small nodes are not hashed, but for simplicity we always hash
            alloy_primitives::keccak256(&rlp)
        } else {
            alloy_primitives::keccak256(&rlp)
        }
    }

    /// Creates an ArenaNode from a proof node.
    ///
    /// Extension nodes should have been merged into branches by TrieNodeV2.
    pub fn from_proof_node(proof_node: ProofTrieNodeV2) -> Self {
        let ProofTrieNodeV2 { node, .. } = proof_node;
        match node {
            TrieNodeV2::EmptyRoot => ArenaNode::Blinded(RlpNode::from_rlp(&[])),
            TrieNodeV2::Leaf(leaf) => {
                ArenaNode::Leaf { key: leaf.key, value: leaf.value, dirty: false }
            }
            TrieNodeV2::Branch(branch) => {
                // Convert branch children to blinded children
                let children: SmallVec<[Option<Index>; 4]> = branch
                    .stack
                    .iter()
                    .map(|_| None) // All children start as blinded (None = not revealed)
                    .collect();

                ArenaNode::Branch(ArenaBranch {
                    children,
                    state_mask: branch.state_mask,
                    short_key: branch.key,
                    dirty: false,
                })
            }
            TrieNodeV2::Extension(_) => {
                panic!("Extension nodes should be merged into branches by TrieNodeV2")
            }
        }
    }
}

/// A branch node in the arena.
#[derive(Debug, Clone)]
pub struct ArenaBranch {
    /// Children indices, packed densely. Use `state_mask` to find the position
    /// for a given nibble.
    pub children: SmallVec<[Option<Index>; 4]>,
    /// Bitmask indicating which of the 16 child slots are occupied.
    pub state_mask: TrieMask,
    /// The short key (extension key) for this branch.
    pub short_key: Nibbles,
    /// Whether this node needs hash recomputation.
    pub dirty: bool,
}

impl ArenaBranch {
    /// Creates a new empty branch.
    pub fn new() -> Self {
        Self {
            children: SmallVec::new(),
            state_mask: TrieMask::default(),
            short_key: Nibbles::default(),
            dirty: true,
        }
    }

    /// Returns the child index at the given nibble position.
    pub fn child_at(&self, nibble: u8) -> Option<Index> {
        if !self.state_mask.is_bit_set(nibble) {
            return None;
        }
        // Count bits below this nibble to find the index
        let mask = TrieMask::new((1u16 << nibble) - 1);
        let idx = (self.state_mask & mask).count_ones() as usize;
        self.children.get(idx).copied().flatten()
    }

    /// Sets the child at the given nibble position.
    pub fn set_child(&mut self, nibble: u8, child: Option<Index>) {
        if self.state_mask.is_bit_set(nibble) {
            // Find existing position
            let mask = TrieMask::new((1u16 << nibble) - 1);
            let idx = (self.state_mask & mask).count_ones() as usize;
            if let Some(slot) = self.children.get_mut(idx) {
                *slot = child;
            }
        } else if child.is_some() {
            // Insert new position
            self.state_mask.set_bit(nibble);
            let mask = TrieMask::new((1u16 << nibble) - 1);
            let idx = (self.state_mask & mask).count_ones() as usize;
            self.children.insert(idx, child);
        }
        self.dirty = true;
    }

    /// Removes the child at the given nibble position.
    pub fn remove_child(&mut self, nibble: u8) {
        if !self.state_mask.is_bit_set(nibble) {
            return;
        }
        let mask = TrieMask::new((1u16 << nibble) - 1);
        let idx = (self.state_mask & mask).count_ones() as usize;
        self.children.remove(idx);
        self.state_mask.unset_bit(nibble);
        self.dirty = true;
    }

    /// Returns an iterator over (nibble, child_index) pairs.
    pub fn iter_children(&self) -> impl Iterator<Item = (u8, Option<Index>)> + '_ {
        ChildIter { state_mask: self.state_mask, children: &self.children, nibble: 0, idx: 0 }
    }

    /// Returns the number of children.
    pub fn num_children(&self) -> usize {
        self.state_mask.count_ones() as usize
    }

    /// Encodes this branch node as RLP.
    pub fn encode_rlp(&self, arena: &NodeArena, buf: &mut Vec<u8>) {
        use alloy_rlp::Header;

        // Collect child RLP encodings
        let mut child_rlps: Vec<Vec<u8>> = Vec::with_capacity(16);
        let mut child_idx = 0;

        for nibble in 0u8..16 {
            if self.state_mask.is_bit_set(nibble) {
                if let Some(Some(index)) = self.children.get(child_idx) {
                    let node = &arena[*index];
                    let rlp = node.compute_rlp(arena);
                    if rlp.len() >= 32 {
                        // Long encoding: hash
                        child_rlps.push(
                            alloy_rlp::encode_fixed_size(&alloy_primitives::keccak256(&rlp))
                                .to_vec(),
                        );
                    } else {
                        // Short encoding: inline
                        child_rlps.push(rlp);
                    }
                } else {
                    // Empty child
                    child_rlps.push(vec![0x80]); // Empty string
                }
                child_idx += 1;
            } else {
                // No child at this position
                child_rlps.push(vec![0x80]); // Empty string
            }
        }

        // Encode as list
        let payload_len: usize = child_rlps.iter().map(|r| r.len()).sum();
        Header { list: true, payload_length: payload_len }.encode(buf);
        for child_rlp in child_rlps {
            buf.extend_from_slice(&child_rlp);
        }
    }
}

impl Default for ArenaBranch {
    fn default() -> Self {
        Self::new()
    }
}

/// Iterator over branch children.
struct ChildIter<'a> {
    state_mask: TrieMask,
    children: &'a SmallVec<[Option<Index>; 4]>,
    nibble: u8,
    idx: usize,
}

impl<'a> Iterator for ChildIter<'a> {
    type Item = (u8, Option<Index>);

    fn next(&mut self) -> Option<Self::Item> {
        while self.nibble < 16 {
            let nibble = self.nibble;
            self.nibble += 1;

            if self.state_mask.is_bit_set(nibble) {
                let child = self.children.get(self.idx).copied().flatten();
                self.idx += 1;
                return Some((nibble, child));
            }
        }
        None
    }
}
