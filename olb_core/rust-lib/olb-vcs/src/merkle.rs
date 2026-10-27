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

/// The CT split: the largest power of two **strictly** less than `n`.
///
/// This single line is what replaces last-leaf duplication. For `n = 3` it is
/// 2, so the tree is `node(node(l0, l1), l2)` and the odd leaf sits one level
/// higher rather than being doubled up — which is why `[a,b,c]` and
/// `[a,b,c,c]` cannot collide.
fn split(n: usize) -> usize {
    debug_assert!(n > 1, "split is only defined for n > 1");
    1usize << (usize::BITS - 1 - (n - 1).leading_zeros())
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
    match leaves.len() {
        0 => hashing::digest(&[]),
        1 => leaves[0],
        n => {
            let k = split(n);
            node(
                domain,
                &root(domain, &leaves[..k]),
                &root(domain, &leaves[k..]),
            )
        }
    }
}

/// Audit path for `index` in a tree of `leaves.len()`, bottom-up.
///
/// Length is at most `ceil(log2(n))`. No direction flags: the verifier
/// recomputes sides from `(index, tree_size)`.
///
/// An out-of-range `index` yields an empty path, which
/// [`verify_inclusion`] then rejects — there is no index for which an empty
/// path verifies in a tree of more than one leaf.
pub fn inclusion_proof(domain: MerkleDomain, leaves: &[[u8; 32]], index: usize) -> Vec<[u8; 32]> {
    let mut out = Vec::new();
    if index < leaves.len() {
        path_into(domain, leaves, index, &mut out);
    }
    out
}

/// RFC 6962 §2.1.1 `PATH(m, D[n])`. The recursive call is emitted before the
/// sibling, so the deepest sibling lands first and the result is bottom-up.
fn path_into(domain: MerkleDomain, d: &[[u8; 32]], m: usize, out: &mut Vec<[u8; 32]>) {
    let n = d.len();
    if n <= 1 {
        return;
    }
    let k = split(n);
    if m < k {
        path_into(domain, &d[..k], m, out);
        out.push(root(domain, &d[k..]));
    } else {
        path_into(domain, &d[k..], m - k, out);
        out.push(root(domain, &d[..k]));
    }
}

/// Which side of each split the leaf falls on, walking DOWN from the root.
/// `true` means the leaf is in the left half, so its sibling is the right
/// subtree root. `sides[0]` is the top split and the last entry is the
/// deepest.
///
/// This is the only thing a verifier needs beyond the siblings, and it is
/// derived entirely from `(leaf_index, tree_size)` — which is why PROPERTY 1
/// costs nothing: no direction flags go on the wire, so none can be forged.
fn sides(leaf_index: usize, tree_size: usize) -> Vec<bool> {
    let mut out = Vec::new();
    let mut i = leaf_index;
    let mut n = tree_size;
    while n > 1 {
        let k = split(n);
        if i < k {
            out.push(true);
            n = k;
        } else {
            out.push(false);
            i -= k;
            n -= k;
        }
    }
    out
}

/// Verify an inclusion proof.
///
/// ```text
/// sides = []; i = leaf_index; n = tree_size
/// while n > 1:                         // walk DOWN, recording sides only
///     k = largest power of two < n
///     if i < k:  sides.push(LEFT);  n = k
///     else:      sides.push(RIGHT); i -= k; n -= k
/// assert len(sides) == len(path)
/// h = leaf_hash                        // then hash UP
/// for (sib, side) in zip(path, reversed(sides)):
///     h = side == LEFT ? node(h, sib) : node(sib, h)
/// assert h == root
/// ```
///
/// **The pseudocode in docs/10 §5.4 is wrong and this is the correction.**
/// That loop descends from the root (`k = split(n)` shrinks `n` from
/// `tree_size`) while consuming `path` front to back — but `path` is
/// bottom-up per RFC 6962 §2.1.1, so it pairs the TOP split with the DEEPEST
/// sibling. It happens to agree for a left-most leaf and disagrees for almost
/// everything else: for `n = 3, i = 1` it computes `node(l1, l0)` where the
/// tree has `node(l0, l1)`. The sides are right, the order they are applied
/// in is not, so they are collected and then applied in reverse. docs/10
/// §5.4 needs the same fix.
///
/// `sides.len() == path.len()` is checked before any hashing, so a short path
/// cannot be padded out to a root and a long one cannot carry extra nodes
/// past it.
///
/// One thing this deliberately does NOT promise: `tree_size` is not fully
/// determined by the proof. `sides(0, 3) == sides(0, 4)`, so a left-most
/// leaf's proof verifies under either size. That is inherent to RFC 6962 (CT
/// derives the same proof length for both) and costs nothing here, because
/// the root is the authoritative binding and the PDA stores `count`
/// alongside it — a size lie that matters is caught against the account, not
/// against the path.
pub fn verify_inclusion(
    domain: MerkleDomain,
    leaf_hash: &[u8; 32],
    leaf_index: usize,
    tree_size: usize,
    path: &[[u8; 32]],
    expected_root: &[u8; 32],
) -> bool {
    if tree_size == 0 || leaf_index >= tree_size {
        return false;
    }
    let s = sides(leaf_index, tree_size);
    if s.len() != path.len() {
        return false;
    }
    let mut h = *leaf_hash;
    for (sib, left) in path.iter().zip(s.iter().rev()) {
        h = if *left {
            node(domain, &h, sib)
        } else {
            node(domain, sib, &h)
        };
    }
    h == *expected_root
}

/// RFC 6962 §2.1.2 consistency proof: evidence that a tree of size `n`
/// extends the tree of size `m` rather than replacing it.
///
/// This is not optional polish. The registry PDA is overwritten on every
/// anchor, so only the newest root is readable on chain — a consistency proof
/// is the ONLY defence a node has against an anchor that quietly rewrites
/// history (docs/10 §5.5).
///
/// `m == 0`, `m > n` and `m == n` all yield an empty proof: the first two are
/// unanswerable requests and the third needs no evidence beyond root equality,
/// which [`verify_consistency`] checks directly.
pub fn consistency_proof(domain: MerkleDomain, leaves: &[[u8; 32]], m: usize) -> Vec<[u8; 32]> {
    let n = leaves.len();
    if m == 0 || m > n || m == n {
        return Vec::new();
    }
    let mut out = Vec::new();
    subproof_into(domain, leaves, m, true, &mut out);
    out
}

/// RFC 6962 `SUBPROOF(m, D[n], b)`.
///
/// `b` is "the root of `D[0:m]` is a node of this subtree, so the verifier
/// already has it". It starts true and is only cleared when the recursion
/// crosses into the right half, which is what lets the proof omit the old
/// root itself whenever `m` falls on a complete-subtree boundary.
fn subproof_into(
    domain: MerkleDomain,
    d: &[[u8; 32]],
    m: usize,
    b: bool,
    out: &mut Vec<[u8; 32]>,
) {
    let n = d.len();
    if m == n {
        if !b {
            out.push(root(domain, d));
        }
        return;
    }
    let k = split(n);
    if m <= k {
        subproof_into(domain, &d[..k], m, b, out);
        out.push(root(domain, &d[k..]));
    } else {
        subproof_into(domain, &d[k..], m - k, false, out);
        out.push(root(domain, &d[..k]));
    }
}

/// Verify that `new_root` (size `n`) extends `old_root` (size `m`).
///
/// Returns false rather than erroring on a nonsensical request (`m == 0`,
/// `m > n`): a caller asking this question is deciding whether to trust an
/// anchor, and "cannot tell" has to land on the same side as "no".
pub fn verify_consistency(
    domain: MerkleDomain,
    old_root: &[u8; 32],
    m: usize,
    new_root: &[u8; 32],
    n: usize,
    proof: &[[u8; 32]],
) -> bool {
    if m == 0 || m > n {
        return false;
    }
    if m == n {
        return proof.is_empty() && old_root == new_root;
    }
    let mut i = 0usize;
    match verify_subproof(domain, m, n, true, proof, &mut i, old_root) {
        // Both roots must come out, and the proof must be exactly consumed:
        // trailing nodes are rejected for the same reason as in
        // `verify_inclusion`.
        Some((rebuilt_old, rebuilt_new)) => {
            i == proof.len() && rebuilt_old == *old_root && rebuilt_new == *new_root
        }
        None => false,
    }
}

/// Mirror of [`subproof_into`]: walks the same recursion and rebuilds both
/// roots from one proof, returning `(root of D[0:m], root of D[0:n])`.
fn verify_subproof(
    domain: MerkleDomain,
    m: usize,
    n: usize,
    b: bool,
    proof: &[[u8; 32]],
    i: &mut usize,
    old_root: &[u8; 32],
) -> Option<([u8; 32], [u8; 32])> {
    if m == n {
        if b {
            // `m` is a complete-subtree boundary: the node is the old root,
            // which the caller supplied, so the proof does not repeat it.
            return Some((*old_root, *old_root));
        }
        let h = *proof.get(*i)?;
        *i += 1;
        return Some((h, h));
    }
    let k = split(n);
    if m <= k {
        // The whole old tree lives in the left half, so its root is
        // unchanged by whatever the right half adds.
        let (old, new_left) = verify_subproof(domain, m, k, b, proof, i, old_root)?;
        let right = *proof.get(*i)?;
        *i += 1;
        Some((old, node(domain, &new_left, &right)))
    } else {
        // The old tree spans the split, so the left half is complete in both
        // trees and one node covers it on both sides.
        let (old_right, new_right) = verify_subproof(domain, m - k, n - k, false, proof, i, old_root)?;
        let left = *proof.get(*i)?;
        *i += 1;
        Some((
            node(domain, &left, &old_right),
            node(domain, &left, &new_right),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Distinct, deterministic leaf data.
    fn d(i: u8) -> [u8; 32] {
        let mut b = [0u8; 32];
        b[0] = i;
        b[31] = 0xA5;
        b
    }

    /// `n` distinct leaf hashes in the anchor domain.
    fn leaves(n: usize) -> Vec<[u8; 32]> {
        (0..n).map(|i| leaf(ANCHOR, &d(i as u8))).collect()
    }

    fn nd(l: &[u8; 32], r: &[u8; 32]) -> [u8; 32] {
        node(ANCHOR, l, r)
    }

    // ---------------------------------------------------------------- 45, 46

    #[test]
    fn empty_root_is_sha256_of_nothing() {
        // docs/10 §6.5 test 45. Fixed by RFC 6962 so an empty tree has a
        // defined root rather than a zero sentinel that collides with
        // "account never written".
        assert_eq!(root(ANCHOR, &[]), hashing::digest(b""));
    }

    #[test]
    fn single_leaf_root_is_the_leaf_and_path_is_empty() {
        // docs/10 §6.5 test 46.
        let id = d(7);
        let l = leaf(ANCHOR, &id);
        assert_eq!(l, hashing::tagged(&[0x00], &id));
        assert_eq!(root(ANCHOR, &[l]), l);

        let path = inclusion_proof(ANCHOR, &[l], 0);
        assert!(path.is_empty());
        assert!(verify_inclusion(ANCHOR, &l, 0, 1, &path, &l));
    }

    // -------------------------------------------------------------------- 47

    #[test]
    fn roots_match_hand_written_ct_split_references() {
        // docs/10 §6.5 test 47. Written out as explicit node() nesting rather
        // than by calling root(), so these do not just restate the
        // implementation's recursion.
        let l = leaves(8);
        let r01 = nd(&l[0], &l[1]);
        let r23 = nd(&l[2], &l[3]);
        let r45 = nd(&l[4], &l[5]);
        let r0_4 = nd(&r01, &r23);

        assert_eq!(root(ANCHOR, &l[..2]), r01);
        // n = 3: k = 2, the odd leaf rides one level up. NOT node(r01, nd(l2, l2)).
        assert_eq!(root(ANCHOR, &l[..3]), nd(&r01, &l[2]));
        assert_eq!(root(ANCHOR, &l[..4]), r0_4);
        // n = 5: k = 4.
        assert_eq!(root(ANCHOR, &l[..5]), nd(&r0_4, &l[4]));
        // n = 7: k = 4, right side is a 3-leaf tree.
        assert_eq!(root(ANCHOR, &l[..7]), nd(&r0_4, &nd(&r45, &l[6])));
        assert_eq!(root(ANCHOR, &l[..8]), nd(&r0_4, &nd(&r45, &nd(&l[6], &l[7]))));

        for n in [2usize, 3, 4, 5, 7, 8] {
            let sub = &l[..n];
            let r = root(ANCHOR, sub);
            for i in 0..n {
                let p = inclusion_proof(ANCHOR, sub, i);
                assert!(
                    verify_inclusion(ANCHOR, &sub[i], i, n, &p, &r),
                    "n={n} i={i}"
                );
            }
        }
    }

    #[test]
    fn split_is_the_largest_power_of_two_strictly_below_n() {
        for (n, k) in [(2, 1), (3, 2), (4, 2), (5, 4), (7, 4), (8, 4), (9, 8), (64, 32)] {
            assert_eq!(split(n), k, "split({n})");
        }
    }

    // -------------------------------------------------------------------- 48

    #[test]
    fn exhaustive_proofs_verify_and_every_single_corruption_fails() {
        // docs/10 §6.5 test 48.
        for n in 1..=64usize {
            let l = leaves(n);
            let r = root(ANCHOR, &l);
            assert!(
                l.len() <= 64 && inclusion_proof(ANCHOR, &l, 0).len() <= 6,
                "path longer than ceil(log2(64))"
            );
            for i in 0..n {
                let p = inclusion_proof(ANCHOR, &l, i);
                assert!(verify_inclusion(ANCHOR, &l[i], i, n, &p, &r), "n={n} i={i}");

                // One flipped bit in one sibling.
                for s in 0..p.len() {
                    let mut bad = p.clone();
                    bad[s][0] ^= 1;
                    assert!(
                        !verify_inclusion(ANCHOR, &l[i], i, n, &bad, &r),
                        "n={n} i={i} sibling {s} corruption accepted"
                    );
                }

                // A lie about which leaf this is.
                if n > 1 {
                    let other = (i + 1) % n;
                    assert!(
                        !verify_inclusion(ANCHOR, &l[i], other, n, &p, &r),
                        "n={n} i={i}: index {other} accepted"
                    );
                }
                assert!(
                    !verify_inclusion(ANCHOR, &l[i], n, n, &p, &r),
                    "n={n}: out-of-range index accepted"
                );

                // A lie about how big the tree is. Rejected for every size
                // that moves any split the leaf passes through. The sizes
                // that survive are exactly those with an identical side
                // sequence (sides(0, 3) == sides(0, 4), for instance) — see
                // verify_inclusion's note on why that is harmless and
                // inherent to RFC 6962. Asserting the equivalence rather
                // than a blanket failure is what makes this test fail if
                // someone drops the tree_size handling entirely.
                let truth = sides(i, n);
                for other in 1..=72usize {
                    if other == n {
                        continue;
                    }
                    if verify_inclusion(ANCHOR, &l[i], i, other, &p, &r) {
                        assert_eq!(
                            truth,
                            sides(i, other),
                            "n={n} i={i}: tree_size {other} accepted with a \
                             DIFFERENT side sequence"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_padded_or_truncated_path_is_rejected() {
        // The loop is driven by tree_size, and leftovers are refused. Without
        // the second half, a proof could carry trailing junk that a relaying
        // peer might reuse.
        let l = leaves(5);
        let r = root(ANCHOR, &l);
        let p = inclusion_proof(ANCHOR, &l, 1);

        let mut long = p.clone();
        long.push(l[0]);
        assert!(!verify_inclusion(ANCHOR, &l[1], 1, 5, &long, &r));

        let short = &p[..p.len() - 1];
        assert!(!verify_inclusion(ANCHOR, &l[1], 1, 5, short, &r));
        assert!(!verify_inclusion(ANCHOR, &l[1], 1, 5, &[], &r));
    }

    // -------------------------------------------------------------------- 49

    #[test]
    fn a_leaf_that_is_not_in_the_tree_never_verifies() {
        // docs/10 §6.5 test 49, first half. PROPERTY 1 + the basic soundness
        // claim: no (index, path) pair in the tree admits an outsider.
        let l = leaves(8);
        let r = root(ANCHOR, &l);
        let outsider = leaf(ANCHOR, b"never committed");
        for i in 0..8 {
            let p = inclusion_proof(ANCHOR, &l, i);
            assert!(
                !verify_inclusion(ANCHOR, &outsider, i, 8, &p, &r),
                "outsider accepted at {i}"
            );
        }
    }

    /// The scheme docs/05 originally asked for, written out so the forgery
    /// below is concrete rather than asserted.
    fn sorted_pair(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
        if a <= b {
            hashing::digest(&[&a[..], &b[..]].concat())
        } else {
            hashing::digest(&[&b[..], &a[..]].concat())
        }
    }

    #[test]
    fn sorted_pair_hashing_loses_the_side_bit_and_we_reject_the_forgery() {
        // docs/10 §6.5 test 49, second half. PROPERTY 1, stated as an attack.
        //
        // Find two leaves whose order matters, then show that a sorted-pair
        // verifier cannot distinguish "x is the left child" from "x is the
        // right child" — the same (leaf, path) pair satisfies both claims.
        // Position-aware hashing does distinguish them, so exactly one of the
        // two claims verifies here.
        let l = leaves(2);
        let (x, y) = (l[0], l[1]);

        assert_eq!(
            sorted_pair(&x, &y),
            sorted_pair(&y, &x),
            "sorted-pair hashing is order-independent by construction"
        );
        assert_ne!(
            nd(&x, &y),
            nd(&y, &x),
            "position-aware hashing must NOT be order-independent"
        );

        let r = root(ANCHOR, &l);
        // The honest claim.
        assert!(verify_inclusion(ANCHOR, &y, 1, 2, &[x], &r));
        // The forged claim: same leaf, same path, swapped position. A
        // sorted-pair verifier accepts it; this one must not.
        assert!(
            !verify_inclusion(ANCHOR, &y, 0, 2, &[x], &r),
            "accepted a leaf at the wrong side of its sibling"
        );
        assert!(!verify_inclusion(ANCHOR, &x, 1, 2, &[y], &r));
    }

    #[test]
    fn an_interior_digest_cannot_be_presented_as_a_leaf() {
        // PROPERTY 2, as the attack actually runs. In a 4-leaf tree
        // nd(l0, l1) is an interior node, and root4 = node(nd(l0,l1),
        // nd(l2,l3)) is also the shape of a 2-leaf tree's root. So an
        // attacker holding the interior digest wants to claim "my commit is
        // leaf 0 of a 2-leaf tree with root root4".
        //
        // They cannot, because the leaf they are allowed to submit is a
        // commit id and the verifier hashes it as leaf(0x00 || id). The 0x00
        // tag means leaf(interior) != interior, so the digest they need is
        // not the digest they get.
        let l = leaves(4);
        let r = root(ANCHOR, &l);
        let interior = nd(&l[0], &l[1]);
        let right = nd(&l[2], &l[3]);

        // The claim as a prover can actually make it: commit id = the
        // interior digest's bytes.
        let as_submitted = leaf(ANCHOR, &interior);
        assert_ne!(as_submitted, interior, "the 0x00 tag is what blocks this");
        assert!(
            !verify_inclusion(ANCHOR, &as_submitted, 0, 2, &[right], &r),
            "an interior digest was laundered through the leaf tag"
        );
        for i in 0..4 {
            let p = inclusion_proof(ANCHOR, &l, i);
            assert!(!verify_inclusion(ANCHOR, &as_submitted, i, 4, &p, &r));
        }

        // Spelled out so the test fails loudly if the leaf tag is ever
        // dropped: WITHOUT it, the raw interior digest does verify. Nothing
        // in the tree shape stops this; only the tag does.
        assert!(
            verify_inclusion(ANCHOR, &interior, 0, 2, &[right], &r),
            "the forgery really is available to an untagged tree"
        );
    }

    // -------------------------------------------------------------------- 50

    #[test]
    fn appending_a_leaf_keeps_every_earlier_leaf_provable() {
        // docs/10 §6.5 test 50. The cumulative-tree guarantee the anchor
        // queue rests on: a leaf index assigned once stays valid forever, and
        // only the path and tree_size move.
        let all = leaves(17);
        for n in 1..=17usize {
            let r = root(ANCHOR, &all[..n]);
            for i in 0..n {
                let p = inclusion_proof(ANCHOR, &all[..n], i);
                assert!(verify_inclusion(ANCHOR, &all[i], i, n, &p, &r), "n={n} i={i}");
            }
        }
        // And a path from the smaller tree is genuinely stale: it does not
        // verify against the larger root. This is why paths are recomputed.
        let old = inclusion_proof(ANCHOR, &all[..3], 0);
        let big = root(ANCHOR, &all[..5]);
        assert!(!verify_inclusion(ANCHOR, &all[0], 0, 5, &old, &big));
    }

    // -------------------------------------------------------------------- 51

    #[test]
    fn consistency_proofs_verify_for_every_m_and_n() {
        // docs/10 §6.5 test 51, positive half.
        let all = leaves(32);
        for n in 1..=32usize {
            let rn = root(ANCHOR, &all[..n]);
            for m in 1..=n {
                let rm = root(ANCHOR, &all[..m]);
                let proof = consistency_proof(ANCHOR, &all[..n], m);
                assert!(
                    verify_consistency(ANCHOR, &rm, m, &rn, n, &proof),
                    "m={m} n={n}"
                );
                assert!(
                    proof.len() <= 2 * 6,
                    "m={m} n={n}: proof of {} nodes",
                    proof.len()
                );
            }
        }
    }

    #[test]
    fn a_rewritten_earlier_leaf_fails_consistency() {
        // docs/10 §6.5 test 51, negative half. This is the whole reason
        // consistency proofs exist here: the PDA holds one root, so a node
        // that only compares roots cannot tell "extended" from "replaced".
        let honest = leaves(8);
        let mut rewritten = honest.clone();
        rewritten[1] = leaf(ANCHOR, b"substituted commit");

        let old = root(ANCHOR, &honest[..3]);
        let new_bad = root(ANCHOR, &rewritten);

        // Proof built from the tampered tree, checked against our old root.
        let p = consistency_proof(ANCHOR, &rewritten, 3);
        assert!(!verify_consistency(ANCHOR, &old, 3, &new_bad, 8, &p));

        // Proof built honestly, checked against the tampered root.
        let p_ok = consistency_proof(ANCHOR, &honest, 3);
        assert!(!verify_consistency(ANCHOR, &old, 3, &new_bad, 8, &p_ok));
        assert!(verify_consistency(
            ANCHOR,
            &old,
            3,
            &root(ANCHOR, &honest),
            8,
            &p_ok
        ));
    }

    #[test]
    fn a_dropped_earlier_leaf_fails_consistency() {
        let honest = leaves(8);
        // Same size, leaf 2 dropped and a new one appended at the end.
        let mut dropped: Vec<[u8; 32]> = honest.clone();
        dropped.remove(2);
        dropped.push(leaf(ANCHOR, b"appended to hide the gap"));

        let old = root(ANCHOR, &honest[..5]);
        let new = root(ANCHOR, &dropped);
        let p = consistency_proof(ANCHOR, &dropped, 5);
        assert!(!verify_consistency(ANCHOR, &old, 5, &new, 8, &p));
    }

    #[test]
    fn consistency_rejects_shrinking_degenerate_and_padded_requests() {
        let all = leaves(8);
        let r8 = root(ANCHOR, &all);
        let r5 = root(ANCHOR, &all[..5]);

        // n < m: the tree got smaller, which is never an extension.
        assert!(!verify_consistency(ANCHOR, &r8, 8, &r5, 5, &[]));
        // m == 0: unanswerable, and must not be a free pass.
        assert!(!verify_consistency(ANCHOR, &r5, 0, &r8, 8, &[]));
        // m == n needs no proof but does need the roots to match.
        assert!(verify_consistency(ANCHOR, &r8, 8, &r8, 8, &[]));
        assert!(!verify_consistency(ANCHOR, &r8, 8, &r5, 8, &[]));
        assert!(!verify_consistency(ANCHOR, &r8, 8, &r8, 8, &[all[0]]));

        // Trailing or missing nodes.
        let p = consistency_proof(ANCHOR, &all, 5);
        let mut long = p.clone();
        long.push(all[0]);
        assert!(!verify_consistency(ANCHOR, &r5, 5, &r8, 8, &long));
        assert!(!verify_consistency(ANCHOR, &r5, 5, &r8, 8, &p[..p.len() - 1]));
        assert!(!verify_consistency(ANCHOR, &r5, 5, &r8, 8, &[]));

        // One flipped bit anywhere in the proof.
        for s in 0..p.len() {
            let mut bad = p.clone();
            bad[s][31] ^= 0x80;
            assert!(!verify_consistency(ANCHOR, &r5, 5, &r8, 8, &bad), "node {s}");
        }
    }

    // -------------------------------------------------------------------- 52

    #[test]
    fn second_preimage_a_tree_of_interior_digests_has_a_different_root() {
        // docs/10 §6.5 test 52. PROPERTY 2. Without the 0x00/0x01 tags these
        // two roots would be equal, and an attacker could present a 2-leaf
        // tree as the 4-leaf one.
        let four = leaves(4);
        let r4 = root(ANCHOR, &four);

        let forged = vec![
            leaf(ANCHOR, &nd(&four[0], &four[1])),
            leaf(ANCHOR, &nd(&four[2], &four[3])),
        ];
        assert_ne!(root(ANCHOR, &forged), r4);

        // Spelled out: what makes them differ is the tag, not luck.
        let untagged = hashing::digest(
            &[
                &[0x01u8][..],
                &nd(&four[0], &four[1])[..],
                &nd(&four[2], &four[3])[..],
            ]
            .concat(),
        );
        assert_eq!(untagged, r4, "the interior shape really is the same");
        assert_ne!(root(ANCHOR, &forged), untagged);
    }

    // -------------------------------------------------------------------- 53

    #[test]
    fn duplicating_the_last_leaf_changes_the_root() {
        // docs/10 §6.5 test 53. PROPERTY 3, CVE-2012-2459. Under the Bitcoin
        // pad-to-power-of-two rule these two are the same tree; under the CT
        // split rule they are not, so a third commit cannot be smuggled in by
        // claiming the tree had four.
        let l = leaves(3);
        let three = l.clone();
        let mut four = l.clone();
        four.push(l[2]);

        assert_ne!(root(ANCHOR, &three), root(ANCHOR, &four));

        // The same thing one level up, where the padding trick is usually
        // applied: 5 leaves vs 5 + a duplicate.
        let l5 = leaves(5);
        let mut l6 = l5.clone();
        l6.push(l5[4]);
        assert_ne!(root(ANCHOR, &l5), root(ANCHOR, &l6));

        // And the duplicate is a real, separately provable leaf rather than
        // padding: both copies verify at their own index.
        let r = root(ANCHOR, &four);
        for i in [2usize, 3] {
            let p = inclusion_proof(ANCHOR, &four, i);
            assert!(verify_inclusion(ANCHOR, &four[i], i, 4, &p, &r));
        }
    }

    // --------------------------------------------------------- domain tags

    #[test]
    fn the_state_domain_cannot_be_confused_with_the_anchor_domain() {
        // docs/10 §3.4: record::state_root calls root(STATE, ..). The leaf
        // tags differ, so a state leaf is not an anchor leaf and a proof does
        // not cross over.
        assert_eq!(STATE.leaf_tag, 0x02);
        assert_eq!(STATE.node_tag, ANCHOR.node_tag);

        let data = b"olb:obj:example";
        assert_ne!(leaf(ANCHOR, data), leaf(STATE, data));

        let a: Vec<[u8; 32]> = (0..4).map(|i| leaf(ANCHOR, &d(i))).collect();
        let s: Vec<[u8; 32]> = (0..4).map(|i| leaf(STATE, &d(i))).collect();
        assert_ne!(root(ANCHOR, &a), root(STATE, &s));

        // Verifying an anchor proof under the state domain fails, and vice
        // versa, because the interior tag is applied by the caller's domain.
        let ra = root(ANCHOR, &a);
        let p = inclusion_proof(ANCHOR, &a, 2);
        assert!(verify_inclusion(ANCHOR, &a[2], 2, 4, &p, &ra));
        assert!(!verify_inclusion(STATE, &s[2], 2, 4, &p, &ra));
    }

    #[test]
    fn state_domain_is_exercised_end_to_end() {
        // record::state_root is another agent's file; this proves the STATE
        // domain works through the same code paths before it lands.
        for n in 1..=12usize {
            let l: Vec<[u8; 32]> = (0..n).map(|i| leaf(STATE, &d(i as u8))).collect();
            let r = root(STATE, &l);
            for i in 0..n {
                let p = inclusion_proof(STATE, &l, i);
                assert!(verify_inclusion(STATE, &l[i], i, n, &p, &r), "n={n} i={i}");
            }
            if n > 1 {
                let proof = consistency_proof(STATE, &l, n - 1);
                assert!(verify_consistency(
                    STATE,
                    &root(STATE, &l[..n - 1]),
                    n - 1,
                    &r,
                    n,
                    &proof
                ));
            }
        }
    }

    #[test]
    fn root_is_order_sensitive() {
        // Leaf order is insertion order (docs/10 §5.2). If the root did not
        // depend on it, "which commit is leaf 3" would be unanswerable.
        let l = leaves(4);
        let mut swapped = l.clone();
        swapped.swap(1, 2);
        assert_ne!(root(ANCHOR, &l), root(ANCHOR, &swapped));
    }

    #[test]
    fn root_is_exactly_32_bytes_and_hex_round_trips() {
        // docs/10 §6.5 test 54, the part that lives in this crate:
        // parse_root in src/bin/run_olb_registry.rs takes 64 hex characters.
        let r = root(ANCHOR, &leaves(9));
        assert_eq!(r.len(), 32);
        let hex = hashing::hex32(&r);
        assert_eq!(hex.len(), 64);
        assert_eq!(hashing::unhex32(&hex).unwrap(), r);
    }
}
