//! RFC 6962 Merkle tree (docs/10 §5.3–§5.5).
//!
//! CONTRACT FILE — signatures are fixed.
//!
//! Three properties this must have, each one chosen against a named attack:
//!
//! - **Position-aware, not sorted-pair.** Sorted-pair hashing (the
//!   OpenZeppelin style docs/05 originally called for) discards the left/right
//!   bit, which makes a path ambiguous and permits forged inclusion proofs for
//!   leaves that are not in the tree. It costs nothing to avoid:
//!   `(leaf_index, tree_size)` already determines every side, so no direction
//!   flags go on the wire.
//! - **Leaf and interior nodes are domain-separated** (`0x00` / `0x01`), which
//!   blocks the classic second-preimage attack of passing an interior digest
//!   off as a leaf.
//! - **The last leaf is never duplicated** to pad to a power of two. That
//!   Bitcoin trick makes `[a,b,c]` and `[a,b,c,c]` produce the same root —
//!   the CVE-2012-2459 class. The CT split rule handles odd `n` and is what
//!   makes the tree cleanly append-only.

use crate::hashing;

/// The only thing that varies between the anchor tree and the state root
/// tree (docs/10 §3.4): the leaf tag. Interior nodes share `0x01`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MerkleDomain {
    pub leaf_tag: u8,
    pub node_tag: u8,
}

/// Anchor tree: leaves are commit ids.
pub const ANCHOR: MerkleDomain = MerkleDomain {
    leaf_tag: 0x00,
    node_tag: 0x01,
};

/// State root tree (docs/10 §3.4). A different leaf tag so a state leaf can
/// never be presented as an anchor leaf.
pub const STATE: MerkleDomain = MerkleDomain {
    leaf_tag: 0x02,
    node_tag: 0x01,
};

/// `SHA256(leaf_tag || data)`.
pub fn leaf(domain: MerkleDomain, data: &[u8]) -> [u8; 32] {
    hashing::tagged(&[domain.leaf_tag], data)
}

/// `SHA256(node_tag || left || right)`.
pub fn node(domain: MerkleDomain, left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut buf = Vec::with_capacity(65);
    buf.push(domain.node_tag);
    buf.extend_from_slice(left);
    buf.extend_from_slice(right);
    hashing::digest(&buf)
}

/// ```text
/// root([])      = SHA256(b"")
/// root([d])     = leaf(d)
/// root(D[0:n])  = node(root(D[0:k]), root(D[k:n]))   where k is the largest
///                 power of two STRICTLY less than n
/// ```
///
/// `leaves` are already-hashed leaf digests.
pub fn root(domain: MerkleDomain, leaves: &[[u8; 32]]) -> [u8; 32] {
    let _ = (domain, leaves);
    todo!("docs/10 §5.3")
}

/// Audit path for `index` in a tree of `leaves.len()`, bottom-up.
///
/// Length is at most `ceil(log2(n))`. No direction flags: the verifier
/// recomputes sides from `(index, tree_size)`.
pub fn inclusion_proof(domain: MerkleDomain, leaves: &[[u8; 32]], index: usize) -> Vec<[u8; 32]> {
    let _ = (domain, leaves, index);
    todo!("docs/10 §5.4")
}

/// Verify an inclusion proof.
///
/// ```text
/// h = leaf_hash; i = leaf_index; n = tree_size
/// for sib in path:
///     k = largest power of two < n
///     if i < k:  h = node(h, sib);   n = k
///     else:      h = node(sib, h);   i -= k; n -= k
/// assert n == 1 && h == root
/// ```
pub fn verify_inclusion(
    domain: MerkleDomain,
    leaf_hash: &[u8; 32],
    leaf_index: usize,
    tree_size: usize,
    path: &[[u8; 32]],
    expected_root: &[u8; 32],
) -> bool {
    let _ = (domain, leaf_hash, leaf_index, tree_size, path, expected_root);
    todo!("docs/10 §5.4")
}

/// RFC 6962 §2.1.2 consistency proof: evidence that a tree of size `n`
/// extends the tree of size `m` rather than replacing it.
///
/// This is not optional polish. The registry PDA is overwritten on every
/// anchor, so only the newest root is readable on chain — a consistency proof
/// is the ONLY defence a node has against an anchor that quietly rewrites
/// history (docs/10 §5.5).
pub fn consistency_proof(domain: MerkleDomain, leaves: &[[u8; 32]], m: usize) -> Vec<[u8; 32]> {
    let _ = (domain, leaves, m);
    todo!("docs/10 §5.5")
}

/// Verify that `new_root` (size `n`) extends `old_root` (size `m`).
pub fn verify_consistency(
    domain: MerkleDomain,
    old_root: &[u8; 32],
    m: usize,
    new_root: &[u8; 32],
    n: usize,
    proof: &[[u8; 32]],
) -> bool {
    let _ = (domain, old_root, m, new_root, n, proof);
    todo!("docs/10 §5.5")
}
