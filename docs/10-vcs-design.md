# 10 — Version-control core: implementable design

Makes `docs/05-version-control.md` buildable. Where docs/05 is ambiguous or
contradicts the real source, this doc decides and says so — every such call is
listed in §7. Nothing here changes privacy semantics or the pins.

Scope: commit format, the DAG, record versioning, three-way merge, Merkle
anchoring, tests. Sync/snapshot transport is `docs/09-sync-design.md`.

Replaces the Phase 0 placeholder in `olb_core/rust-lib/src/lib.rs`
(`OlbCore::commit`, `store::`, `verify`) — a flat `Vec` of commits with a single
`parent` string and no canonical encoding.

---

## 0. Crate layout

| Crate | Used by | Contents |
| --- | --- | --- |
| `crates/olb-merkle` | `olb_core`, LEZ guest, host runner | `no_std` + `sha2` only. Tree, root, inclusion and consistency proofs. |
| `crates/olb-vcs` | `olb_core`, host runner | `canon`, commit header, record versioning, DAG, merge. Needs `serde_json`. |

**Build constraint, confirmed:** `olb_core/flake.nix` declares `src = ./.`, so
only the `olb_core/` directory enters the nix store. A path dependency on
`../../crates/olb-merkle` will not resolve under `nix build .#lgx`. Until that
is proven otherwise, `olb_core/rust-lib/src/merkle.rs` is a **committed copy**
of `crates/olb-merkle/src/lib.rs`, kept byte-for-byte in step and checked by a
test that `include_str!`s both and compares — the same discipline
`methods/guest/src/bin/olb_registry.rs` and `src/lib.rs` already use for the
program body. `olb-vcs` does not cross into the guest, so it lives only in
`olb_core/rust-lib/src/vcs/` for now. Revisit if a later `src = ./..` layout
survives scaffold's module discovery (CLAUDE.md repository layout + rule 9:
`git add` before every build).

---

## 1. Commit format

### 1.1 Canonical serialization

Every hash and signature in OLB is taken over `canon(value)`: RFC 8785 (JCS),
restricted so it is cheap to get right.

```
canon:
  objects  -> keys sorted ascending by UTF-16 code unit, no whitespace,
              {"k":v,...}
  arrays   -> order preserved, [v,...]
  strings  -> NFC-normalized, minimal JSON escaping: \" \\ \b \f \n \r \t and
              \u00XX for the remaining C0 controls; everything else literal
              UTF-8. Lone surrogates are an error, not replaced.
  integers -> i64/u64, shortest decimal, no "+", no leading zeros, "-0" is an
              error
  bool/null-> true | false | null
  floats   -> ERROR
```

**No IEEE floats anywhere in hashed OLB content.** This is the one deliberate
narrowing of JCS and it removes its single hardest requirement (reproducing
ECMAScript `Number::toString`). Counts and versions are `u64`; timestamps are
`i64` milliseconds; measured quantities are decimal **strings** with a separate
`unit`, which `docs/05` §1.1 `parameters[] {name, value, unit, source}` already
implies. `canon` returns `Err(CanonError::Float)` rather than encoding lossily,
so a float can never reach a hash silently.

`canon` is implemented over its own `Canon` value type, not over
`serde_json::Value`, so the type system enforces the no-float rule.

### 1.2 Why the current code is not canonical

`OlbCore::commit` hashes and signs `serde_json::to_vec(&body)`.
With `serde_json` 1.0.149 and the lockfile as committed (no `indexmap` in
`olb_core/rust-lib/Cargo.lock`), `serde_json::Map` is a `BTreeMap`, so keys
*happen* to come out sorted. The ids are stable by accident, and four things
break them with no error anywhere:

1. **Feature unification.** Any crate in the graph enabling `serde_json`'s
   `preserve_order` flips `Map` to `IndexMap` for the whole build. Key order
   becomes `json!` insertion order, every id changes, and nothing fails to
   compile. This is the real hazard: a transitive SDK bump can rewrite history.
2. **Floats.** `serde_json` emits shortest-roundtrip; JCS requires ECMAScript
   formatting. They disagree at the exponent boundaries (`1e100` vs `1e+100`),
   and nothing stops a `1.0` that serializes as `1.0` on one path and `1` on
   another.
3. **Unicode.** No NFC normalization, so `é` typed two ways gives two ids for
   the same visible text.
4. **No versioning of the encoding.** Nothing in the preimage says which
   encoding produced it.

Also: `record::signed` puts `v` and `author` *outside* the signature.
`verify()` checks the signature with the embedded `author` key, so swapping
`author` fails verification — but `v` is unauthenticated and strippable, which
is a version-rollback lever. In the format below, `v` is inside the hashed
header.

### 1.3 Commit header (hashed and signed)

```jsonc
{
  "v": 1,
  "project": "olb:proj:<base32>",
  "parents": ["<64 hex>", "..."],   // 0 = root, 1 = normal, 2 = merge
  "tree": "<64 hex>",               // state root after this commit, §3.4
  "author": "<64 hex ed25519 pk>",
  "co_signers": ["<64 hex pk>"],    // approvers / agent operator; sorted, unique
  "message": "Run 14: add plate 3 melt curves",
  "changes": [
    { "object": "olb:obj:<base32>", "from_version": 2, "to_version": 3,
      "kind": "create|edit|flag|restore|merge", "record": "<64 hex>" }
  ],
  "files_added": ["logos:<cid>"],   // sorted, unique
  "files_removed": [],
  "timestamp": 1761480000000        // i64 ms UTC
}
```

`changes` is sorted by `object`. `record` is the content hash of the resulting
record version — it is what makes a commit verifiable without trusting the
record store.

### 1.4 Id and signatures

```
preimage  = b"olb.commit.v1\x00" || canon(header)
commit_id = SHA256(preimage)                      // hex; short form = [..7]
sig       = Ed25519_sign(author_sk, preimage)
co_sig_i  = Ed25519_sign(cosigner_sk,
              b"olb.commit.cosign.v1\x00" || commit_id_bytes)
```

The author signs the preimage directly (ed25519 hashes internally; no
hash-then-sign ambiguity). Co-signers sign the 32-byte id, which they only
produce after recomputing the preimage themselves — cheap, and it binds them to
the exact content.

### 1.5 Stored envelope (not hashed)

```jsonc
{ "id": "...", "header": { ... }, "sig": "<128 hex>", "co_sigs": ["<128 hex>"],
  "meta": { "branch": "main", "received_at": 0, "gen": 14,
            "anchor": { "leaf": 1203, "batch": 212 } } }
```

`meta` is local, mutable, never hashed, never trusted from a peer.

**`branch` is not in the header.** docs/05 §2 puts `"branch": "main"` in the
commit. A commit's branch is not a property of the commit — it changes when the
branch is renamed, merged or deleted — and hashing it means the same content
gets different ids on different branches, so a merge can never recognise an
identical commit. `branch` becomes a display hint in `meta`.

---

## 2. The DAG

### 2.1 Refs and HEAD

| Thing | Storage | Rules |
| --- | --- | --- |
| commit | `commits` table, keyed by id | append-only, content-addressed, never deleted |
| branch | `refs` row `refs/heads/<name>` -> commit id | mutable pointer; `main` always exists, protected |
| tag | `refs` row `refs/tags/<name>` -> `{commit, tagger, sig}` | immutable: re-tagging an existing name is an error |
| HEAD | `refs` row `HEAD` -> `ref:refs/heads/<name>` or a raw id | symbolic by default; raw id = detached (read-only in the UI) |

Deleting a branch deletes only the ref. Commits are unreachable, not gone —
history is never rewritten, and a tag or another branch can still reach them.

### 2.2 Tables (SQLite, the Phase 1b target for `store::`)

```sql
commits(id TEXT PK, project TEXT, tree TEXT, author TEXT, ts INTEGER,
        gen INTEGER, canon BLOB, sig BLOB, header_json TEXT)
commit_parents(child TEXT, idx INTEGER, parent TEXT, PRIMARY KEY(child, idx))
commit_cosigs(commit TEXT, signer TEXT, sig BLOB, PRIMARY KEY(commit, signer))
refs(name TEXT PK, kind TEXT, target TEXT, sig BLOB)
records(hash TEXT PK, object TEXT, version INTEGER, body BLOB)
                                      -- INDEX(object, version), NOT unique
record_changes(commit TEXT, object TEXT, from_version INTEGER,
               to_version INTEGER, kind TEXT, record TEXT)
pending(id TEXT PK, envelope_json TEXT, missing_parent TEXT)
anchor_leaves(seq INTEGER PK AUTOINCREMENT, commit TEXT UNIQUE)
anchors(batch INTEGER PK, root BLOB, count INTEGER, lez_tx TEXT, at INTEGER)
```

`canon` is stored so a signature can be re-verified without re-serializing
(and so a peer's bytes can be replayed exactly).

### 2.3 Parent order

`parents[0]` is the **target** side of a merge — the branch being merged into.
The first-parent chain is therefore the mainline, and it is what `vcs.log`
assigns lane 0 (S6 requires lane 0 to be the spine). `parents[1..]` are the
merged-in heads, sorted by id for determinism.

### 2.4 Generation numbers and merge base

`gen = 1 + max(gen of parents)`, `gen(root) = 0`, stored in `commits.gen`.
Merge base = best common ancestor, computed by the standard two-colour paint
with a max-heap keyed by `(gen, id)`: push both heads, pop highest, propagate
colours to parents, a commit coloured by both and not yet superseded is a
candidate; stop when the heap holds only commits already coloured by both.

**Multiple merge bases (criss-cross) are not merged recursively.** Git's
`-s recursive` merges the bases into a virtual base; we do not. If the paint
returns more than one base, every object whose content differs across the bases
is reported as a conflict and resolved by hand. Simpler, auditable, and a lab
record is not a place to synthesise a base nobody wrote.

### 2.5 Receiving a commit

Validation, in order (docs/05 §5), all deterministic and identical on every
node:

1. `canon(header)` round-trips and re-hashes to the claimed `id`.
2. `sig` verifies over the preimage.
3. `author` is a project member at the merge base/parent state with a role that
   may write (`docs/04`).
4. every `parents[i]` is known — else the envelope goes to `pending` keyed by
   the first missing parent and is retried when that parent lands.
5. `changes[].from_version` equals the current head version of that object on
   the parent state; `to_version == from_version + 1`.
6. every `changes[].record` blob is present, hashes correctly, and its internal
   `version` matches `to_version`.
7. recomputed `tree` equals the header `tree`.
8. if the commit is a merge into a protected branch: enough valid, non-stale
   co-signatures (§4.5).

Any failure -> dropped and logged, never applied. A pending commit never
blocks unrelated commits.

---

## 3. Record versioning

### 3.1 Version identity

```
record_hash = SHA256(b"olb.record.v1\x00" || canon(record))
object_id   = "olb:obj:" || base32_nopad_lower(record_hash of version 1)
```

So an object's identity *is* the hash of its first version, exactly as docs/05
§1 says. A useful consequence: two members cannot independently "create the
same object with different content" — different content gives a different hash
gives a different `olb:obj:` id, so creation never conflicts.

### 3.2 `prev`: the version chain

docs/05 §1 has one `parents[]` on a record and uses it for derivation
(`derived_from`, idea linkage). Version lineage is a different relation, so the
record gains:

```jsonc
"version": 3,
"prev": ["<record_hash of v2>"]   // [] for v1; 2+ entries for a merged version
```

`prev` is a `Vec` from day one because a merged record version genuinely has two
predecessors (§4). `parents[]` keeps its docs/05 meaning (derivation) and is not
touched.

### 3.3 Reconstructing a chain — two independent paths that must agree

**From commits:** walk the commit DAG back from a ref, collect every
`changes[]` entry with `object == id`, order by `to_version`. Each entry names
the `record` hash, so each version's blob is fetched by hash, not by index.
This gives *who changed it, when, in which commit*.

**From blobs:** load the head version's blob, follow `prev` hashes to version 1,
checking at each step that the hash of the loaded blob equals the `prev` entry
that pointed at it. This gives *the content chain*, and it verifies with no
commit data at all.

`vcs.history(objectId)` computes both and errors if they disagree — that
disagreement is exactly what a tampered store or a buggy apply looks like.
Checks: versions dense and strictly increasing from 1; each commit's
`from_version` equals the previous entry's `to_version`;
`blob.version == change.to_version`.

Divergence is legal in the store: two branches can each produce a v3, so
`records` is keyed by `hash` with a **non-unique** index on
`(object, version)`. Neither overwrites the other; the merge picks or merges.

### 3.4 `tree` — the state root

docs/05 has no tree, only `changes[]`. That is not enough: two commits with
identical change lists on divergent histories produce different state, snapshot
bootstrap has nothing to verify against, and merge has to replay the whole DAG
to know what a branch's state is. So each commit carries:

```
leaves  = for each live object, sorted by object_id:
            SHA256(0x02 || canon([object_id, version, record_hash]))
tree    = merkle_root(leaves)        // same code as §5, different leaf tag
```

Sorted by `object_id`, because this tree is rebuilt per commit rather than
appended to. Computing it is O(live objects) per commit; for the sizes OLB sees
(hundreds to low thousands of records per project) that is cheap, and a cached
incremental version is a later optimisation, not a format change.

### 3.5 Restore

`vcs.restore(commitId, recordId?)` reads version K, writes a new version
`N+1` whose fields equal K's, with `prev = [hash(vN)]` and
`kind: "restore"` plus `restored_from: K` in the change entry. Version N stays
readable. Matches the S6 footnote: "Restoring creates a new commit. Nothing in
the history is ever rewritten."

### 3.6 There is no delete

Records are append-only, so "removed" in a diff summary means a new version
with `flags.retracted = true`. A delete/edit conflict therefore reduces to an
ordinary field conflict on `flags.retracted`.

---

## 4. Merge

### 4.1 Object level

```
base = merge_base(ours_head, theirs_head)      // >1 base -> §2.4
for id in union(objects(base), objects(ours), objects(theirs)):
    b, o, t = version of id at base / ours / theirs   (Option)
    if o == t                  -> take o            (same record_hash)
    else if o == b             -> take t
    else if t == b             -> take o
    else                       -> field-level merge of (b, o, t)
```

Comparison is by `record_hash`, so "unchanged" is exact and free.

### 4.2 Field level

For each key in `union(keys(b), keys(o), keys(t))`:

| Case | Result |
| --- | --- |
| `o == t` | take it (including both sides making the identical edit) |
| `o == b` | take `t` |
| `t == b` | take `o` |
| all three differ | **conflict** — never auto-resolved |

docs/05 §3 is explicit: "never auto-merge conflicting fields." Specifics:

- **Text fields** (`observations`, `summary`, `notes`, `statement`) are opaque
  at merge time. No three-way text merge, not even for non-overlapping lines.
  docs/05 §3's "text fields diffed by line/word" is a **diff/presentation**
  rule for S6, not a merge rule; auto-stitching two scientists' prose can
  fabricate an observation neither wrote.
- **Attachments / files** merge as a **set keyed by `cid`**. Adds from both
  sides union cleanly; order is canonical (sorted by `cid`) so order is never a
  conflict. Remove on one side + metadata edit on the other for the same `cid`
  is a conflict.
- **Keyed arrays** merge element-wise by their natural key — `predictions[]` by
  `label`, `milestones[]` by `id`, `authors[]` by `key`, `reviewers[]` by `key`
  — recursing into the element's fields. Both sides adding the same key with
  different content conflicts on that element only.
- **Unkeyed arrays** (`stages_enabled[]`, `reveals[]`) are opaque values: both
  sides changing them differently is a conflict.

### 4.3 Conflict shape on the wire (JSON string, docs/08 R10)

```jsonc
{ "object": "olb:obj:...", "recordTitle": "Run 11b",
  "kind": "field|element|set",
  "field": "fields.observations",       // dotted path, element key appended
  "base": "...", "ours": "...", "theirs": "..." }
```

Resolution comes back as
`[{ "object": ..., "field": ..., "choose": "ours|theirs|custom", "value": ... }]`
covering **every** reported conflict; a partial resolution is rejected.
Applying it writes the merged record versions (`prev = [ours_hash,
theirs_hash]`, `version = max(ours, theirs) + 1`) and then **one** merge commit
with `parents = [target_head, source_head]`.

### 4.4 Merge request

Fixture `history.mergeRequest` plus the fields it needs to be sound:

```jsonc
{ "id": "mr-h2", "project": "olb:proj:...",
  "title": "Add H2 draft and docking protocol",
  "source": "h2-computational", "target": "main",
  "author": "<pk>",
  "base": "<commit id>",          // merge base pinned at open time
  "source_head": "<commit id>",   // heads the summary was computed against
  "target_head": "<commit id>",
  "reviewers": [{ "key": "<pk>", "name": "Priya Raman",
                  "state": "requested|approved|changes_requested|declined" }],
  "approvals": [{ "key": "<pk>", "source_head": "...", "target_head": "...",
                  "sig": "<128 hex>" }],
  "approvals_needed": 1,          // copied from project settings at open time
  "summary": { "added": 3, "edited": 1, "removed": 0, "conflicts": 0 },
  "state": "open|merged|closed" }
```

An approval signs

```
b"olb.mr.approve.v1\x00" || mr_id || source_head_bytes || target_head_bytes
```

so it cannot be replayed onto another MR or onto a different pair of heads.
**An approval goes stale the moment either head moves** and stops counting;
without that, "get approved, then push" defeats review entirely. docs/05 does
not mention this.

`approvals_needed` is snapshotted at open time so changing the project setting
mid-review cannot lower an in-flight bar.

### 4.5 Enforcing the threshold

CLAUDE.md constraint 7: LEZ shared private accounts are 1-of-n at the key
layer, so a threshold is never enforced by who holds a key. Three layers, and
only the last two matter:

1. **Local UI gate** in `olb_core` — advisory only. Any member's own node could
   lie to itself.
2. **Commit validation** — authoritative for history. A merge commit into a
   protected branch is accepted by `apply_commit` only if its `co_signers` /
   `co_sigs` contain at least `approvals_needed` valid signatures from
   **distinct** keys that are all (a) project members at the **merge base**
   state, (b) in a role permitted to approve, and (c) not the merge author.
   This rule is deterministic and identical on every node, so a merge commit
   lacking the signatures is dropped by every honest peer — the threshold holds
   without any single node being trusted.
3. **LEZ program logic** — authoritative for money. Any spend gated on
   "2 of 3 reviewers" is checked inside `olb_funding`, counting signatures over
   the milestone id in the guest, never by a shared account's key.

Eligibility is evaluated against membership **at the merge base**, so a
membership change landing mid-review cannot retroactively validate or
invalidate an approval. Reviewers cannot approve merges (docs/05 §3
Protection), so role `reviewer` is excluded from the eligible set.

### 4.6 Fast-forward

If `target_head` is an ancestor of `source_head` the ref can move with no merge
commit. The approval gate is on the **merge operation**, not on the existence
of a merge commit, so a fast-forward into a protected branch still requires
`approvals_needed`. Implementation: on a protected target, do not
fast-forward — always write a merge commit, so the co-signatures have somewhere
to live and the rule in §4.5 can be checked by peers. On an unprotected target,
fast-forward freely.

---

## 5. Merkle anchoring

### 5.1 What the deployed program actually takes

`methods/guest/src/bin/olb_registry.rs` is
`anchor(registry, authority, project_tag: String, root: [u8;32], count: u64)`,
writing 40 bytes `count LE || root` into a PDA seeded
`[r#const("olb_registry"), arg("project_tag")]` — **one slot per project**.

One gap against docs/05 §4 remains:

- docs/05 says **"sorted pair hashing"**. Rejected — see §5.3.

Resolved 26 Oct 2026: the program took no `project_tag` until then, so every
project in a deployment shared a single 40-byte slot. `spel` turned out to
support `const` / `account` / `arg` seeds already
(`spel-framework-macros/src/lib.rs:757`), so this was a seed change, not a
redesign. `project_tag = H(project_id || project_salt)` is what keeps project
identity off-chain, and it was a Phase 2 blocker; fixing it before Phase 1b
anchors real history cost one `program_id` and one orphaned test anchor.

### 5.2 Cumulative tree, not per-batch

Each project's slot is overwritten on every anchor, so that project's anchored
root is a **cumulative, append-only tree over every commit anchored so far**,
not a per-batch tree.
`count` is the total number of leaves, so the newest `(count, root)` pair is
sufficient to verify **any** earlier commit, given its leaf index. A per-batch
tree would leave every root but the last unverifiable the moment the next batch
lands.

Leaf order is **insertion order**: the `anchor_leaves.seq` assigned when a
commit is queued for anchoring. Not sorted by id, because sorting would
reshuffle the tree on every append. The queue order is the deterministic
topological order of the DAG — `(gen, timestamp, id)` — so two nodes holding
the same commits build the same tree.

### 5.3 Tree spec (RFC 6962)

```
leaf_i  = SHA256(0x00 || commit_id_bytes)            // 32 raw bytes, not hex
node    = SHA256(0x01 || left || right)
root([])  = SHA256(b"")
root([d]) = leaf(d)                                  // n == 1
root(D[0:n]) for n > 1:
    k = largest power of two strictly less than n
    SHA256(0x01 || root(D[0:k]) || root(D[k:n]))
```

- **Position-aware, not sorted-pair.** Sorted-pair hashing (OpenZeppelin style)
  throws away the left/right bit. That makes a path ambiguous, permits forged
  inclusion proofs for leaves that are not in the tree, and breaks multiproofs.
  It is also unnecessary here: `(leaf_index, tree_size)` already determines
  every side, so no direction flags go on the wire.
- **Leaf and internal nodes are domain-separated** (`0x00` / `0x01`), which
  blocks the classic second-preimage attack of passing an internal digest off
  as a leaf.
- **Never duplicate the last leaf** to pad to a power of two (the Bitcoin
  trick). It makes `[a,b,c]` and `[a,b,c,c]` collide — the CVE-2012-2459 class.
  The CT split rule above handles odd `n` and is what makes the tree cleanly
  append-only.
- The state root (§3.4) reuses this code with leaf tag `0x02` and node tag
  `0x01`; `MerkleDomain { leaf_tag, node_tag }` is the only parameter.

### 5.4 Inclusion proof

```jsonc
{ "commit": "<64 hex>", "leaf_index": 1203, "tree_size": 4096,
  "path": ["<64 hex>", "..."], "batch": 212, "root": "<64 hex>" }
```

Verify:

```
sides = []; i = leaf_index; n = tree_size
while n > 1:                          # walk DOWN, recording sides only
    k = largest power of two < n
    if i < k:  sides.push(LEFT);  n = k
    else:      sides.push(RIGHT); i -= k; n -= k
assert len(sides) == len(path)        # before hashing anything
h = SHA256(0x00 || commit_id_bytes)   # then hash UP
for (sib, side) in zip(path, reversed(sides)):
    h = side == LEFT ? SHA256(0x01 || h || sib) : SHA256(0x01 || sib || h)
assert h == anchored_root
```

Path length is at most `ceil(log2(n))`.

**Corrected 27 Oct 2026.** The previous version of this loop descended from
the root while consuming `path` front to back, which pairs the **top** split
with the **deepest** sibling — `path` is bottom-up per RFC 6962 §2.1.1. It
agrees for a left-most leaf and disagrees for almost everything else: at
`n = 3, i = 1` it computes `node(l1, l0)` where the tree holds
`node(l0, l1)`. The sides it derived were right; the order they were applied
in was not. Found by implementing it verbatim and watching seven tests fail.

Checking `len(sides) == len(path)` before hashing is not tidiness: without
it a short path can be padded out to a root, and a long one can carry extra
nodes past it, which is proof malleability a relaying peer can exploit.

One thing inclusion deliberately does not promise: `tree_size` is not fully
bound by the proof, because `sides(0, 3) == sides(0, 4)` — a left-most leaf
verifies under either. That is inherent to RFC 6962 and harmless here, since
the root is the authoritative binding and the PDA stores `count` beside it.

**Proofs are recomputed on demand from `anchor_leaves`, not stored.** docs/05 §4
says "store each commit's Merkle path with the batch number" — that is wrong for
a cumulative tree: an older leaf's path *grows* as the tree grows, so a stored
path is stale after the next anchor. Store the **leaf index** and the batch;
recompute the path (microseconds at OLB's scale). `anchor.verify(commitId)`
returns `{valid, batch, root, leafIndex}`.

### 5.5 Consistency proof

A node must be able to check that a newly anchored root really **extends** the
one it saw, rather than replacing history. RFC 6962 §2.1.2 consistency proofs
do that, and with a single overwritten PDA they are the only defence against a
rewritten anchor. `olb-merkle` implements `consistency_proof(m, n)` and
`verify_consistency(old_root, m, new_root, n, proof)`; `olb_core` runs it on
every observed root change and raises an amber state in S1/S6 if it fails.

### 5.6 Flush

Policy from project settings (docs/05 §4): every change / 5 min or 100 commits
(default) / daily / key moments. Pre-registration, milestone release and
publication always flush immediately. `anchor` is refused for `count == 0`.
`RISC0_DEV_MODE=1` locally; never ship dev-mode proofs to testnet.

---

## 6. Tests to write

CLAUDE.md requires tests for record versioning, the commit DAG and merge,
Merkle proofs, invite sealing and key rotation. The last two are `docs/04`.

### 6.1 Canonical form, id, signatures

1. `canon` sorts keys by UTF-16 code unit — ASCII, non-ASCII, case-differing
   keys, nested objects, objects inside arrays.
2. One header built with three different field insertion orders yields one
   identical `commit_id`.
3. **Committed known-answer vectors** (`fixtures/vcs-vectors.json`): fixed
   headers -> hard-coded ids. This is the regression net for the
   `preserve_order` hazard in §1.2 and must fail loudly if the encoding moves.
4. `canon` rejects any float, non-finite, or `-0`; returns `Err`, never a lossy
   encode.
5. Strings: C0 controls escaped, `"`/`\` escaped, astral/emoji pass through
   unescaped, lone surrogate rejected, NFC vs NFD inputs normalize to the
   **same** id (and a test documents that the writer normalizes before hashing).
6. `sig` verifies over the preimage; flipping one byte of `message`, one
   `parents` entry, or the `author` key each break verification.
7. Mutating `meta.branch` does not change the id; mutating any header field
   does.
8. Co-signatures verify under the cosign domain; the same key twice counts once;
   a co-signature for commit A does not verify for commit B.

### 6.2 Record versioning

9. create -> v1, `prev == []`, and `object_id` equals base32 of v1's hash.
10. three sequential edits -> versions 1..4 dense; `prev` walks back to v1 and
    every `prev` hash matches the loaded blob.
11. **Two-way agreement:** the chain reconstructed from commit `changes[]`
    equals the chain reconstructed by following `prev`.
12. restore v2 from v4 -> v5 with v2's fields, `prev == [hash(v4)]`, kind
    `restore`, and v4 still readable.
13. reject a commit whose `from_version` is not the current head version.
14. a commit whose `changes[].record` blob is missing goes to `pending`, is not
    applied, and applies cleanly once the blob arrives.
15. two branches each produce a v3 of the same object -> both blobs stored,
    `(object, 3)` returns two rows, neither overwrites.
16. `tree` recomputed from the record store equals the stored `tree`; changing
    the store's iteration order does not change it.
17. retraction: `flags.retracted = true` is a new version, and the previous
    version is still readable.

### 6.3 DAG

18. root commit has `parents == []`; exactly one root per project.
19. `gen` increments along a chain; `merge_base(a, a) == a`;
    `merge_base(child, ancestor) == ancestor`.
20. fork + merge: merge commit has two parents, `parents[0] == target_head`, and
    the first-parent walk yields only the mainline.
21. criss-cross: `merge_base` returns two bases -> merge reports conflicts
    rather than silently resolving.
22. out-of-order receipt: child delivered before parent is held in `pending` and
    applied when the parent lands; the store never holds a commit with an
    unknown parent.
23. a permanently orphaned commit stays pending forever, is reported as pending,
    and does not block unrelated commits.
24. `vcs.branch(name, fromCommit)` creates a ref with no commit; two branches at
    one commit are independent; deleting a branch deletes no commits and a tag
    still reaches them.
25. re-tagging an existing tag name errors; tags survive branch deletion.
26. direct (non-merge) commit onto protected `main` with `mergeApprovals = 1` —
    **allowed** (docs/05 protects *merges*; see §7).
27. `vcs.log` lanes are deterministic for a fixed DAG, lane 0 is the first-parent
    chain, and a 4th lane collapses to a `+n branches` marker (S6).

### 6.4 Merge

28. disjoint objects -> auto-merge, no conflicts, merged tree is the union.
29. disjoint fields of one object -> auto-merge, both changes present,
    `version == max+1`, `prev` holds both hashes.
30. same field, same new value both sides -> auto-merge, no conflict.
31. same field, different values -> exactly one conflict with base/ours/theirs;
    nothing applied until resolved.
32. one side untouched since base -> take the other, no conflict.
33. identical record content on both sides via different commits -> zero
    conflicts.
34. attachments: adds on both sides union by `cid`; remove-on-ours +
    metadata-edit-on-theirs for the same `cid` -> conflict.
35. keyed array `predictions[]`: add P3 / add P4 -> both present, sorted by
    label; both add P3 with different text -> conflict scoped to element P3.
36. text field changed on both sides -> conflict even when the changed lines do
    not overlap (documents the deliberate refusal).
37. partial resolution is rejected; a full resolution produces exactly one merge
    commit.
38. approvals: needed = 2 with 1 valid -> refused; 2 valid -> accepted; 2 where
    one is the merge author -> refused; 2 where one holds role `reviewer` ->
    refused; 2 from the same key -> refused.
39. staleness: approve, push to `source`, merge -> the approval does not count
    and the merge is refused.
40. an approval signature for mr-A does not verify for mr-B, nor for a different
    `(source_head, target_head)` pair.
41. a peer receiving a merge into protected `main` with too few valid
    co-signatures **drops** it — the peer rule and the local gate agree.
42. membership change after the merge base does not change approver eligibility.
43. fast-forward into a protected branch still writes a merge commit and still
    requires approvals; into an unprotected branch it moves the ref.
44. ours retracts / theirs edits a field -> conflict on `flags.retracted`.

### 6.5 Merkle

45. `root([]) == SHA256(b"")`; `anchor` refuses `count == 0`.
46. `n == 1`: root is `SHA256(0x00 || id)`, path is empty, proof verifies.
47. `n` in {2,3,4,5,7,8}: root matches a hand-written reference computed from
    the CT split rule, and every leaf's proof verifies.
48. exhaustive: for every `n` in 1..=64 and every leaf, the proof verifies;
    corrupting one sibling byte, the `leaf_index`, or the `tree_size` each make
    it fail.
49. a proof for a leaf that is not in the tree fails — including the
    sorted-pair forgery, constructed explicitly, to prove position-aware hashing
    rejects it.
50. append-only: proofs for leaves 0..n still verify against the root over
    0..n+1 with the new `tree_size`.
51. consistency proof between sizes `m < n` verifies; a root that drops or
    rewrites an earlier leaf fails it.
52. second preimage: a 4-leaf tree and a 2-leaf tree whose leaves are the
    4-leaf tree's internal digests do **not** share a root.
53. no duplicate-last-leaf collision: `[a,b,c]` and `[a,b,c,c]` differ.
54. program shape: the root is exactly 32 bytes, `count` fits `u64`,
    `parse_root(hex(root))` in `src/bin/run_olb_registry.rs` succeeds, and
    `show` reads back `count LE || root`.
55. the guest copy and the `olb_core` copy of `merkle.rs` are byte-for-byte
    identical, and both produce the vectors in `fixtures/vcs-vectors.json`
    (cross-toolchain determinism: host, module staticlib, risc0 guest).

---

## 7. Where docs/05 is underspecified — judgment calls

| # | docs/05 says | Call made | Why |
| --- | --- | --- | --- |
| 1 | "Canonical JSON (RFC 8785)" | JCS **minus floats**; integers, decimal strings and `unit` only | Reproducing ECMAScript `Number::toString` is the hardest part of JCS and the only one with no safe failure mode. A rejected float is a bug; a mis-encoded float is silent history corruption. |
| 2 | commit carries `"branch": "main"` | `branch` moved out of the hashed header into `meta` | A branch is a mutable label, not commit content. Hashing it makes identical content hash differently per branch and breaks merge's "same commit" test. |
| 3 | no tree / state root | added `tree` to the header | Without it, divergent histories with identical change lists are indistinguishable, snapshots have nothing to verify against, and merge must replay the DAG. |
| 4 | record `parents[]` | kept for derivation; added `prev[]` for the version chain | docs/05 overloads one field for two relations. A merged version has two predecessors, so `prev` is a `Vec`. |
| 5 | "sorted pair hashing" | RFC 6962 position-aware hashing, domain-separated leaves, no last-leaf duplication | Sorted pairs discard the left/right bit, which enables forged inclusion proofs and breaks multiproofs. Costs nothing: `(index, size)` already give the sides. |
| 6 | per-batch root; "store each commit's Merkle path" | one **cumulative** append-only tree per project; store the **leaf index**, recompute paths | The program overwrites each project's PDA, so only that project's newest root is readable on-chain — a per-batch root is unverifiable the moment the next batch lands, and an older leaf's path grows as the tree grows. |
| 7 | `anchor(project_tag, root, count)` | matches, as of 26 Oct 2026 | Originally a mismatch — the program had no tag and all projects shared one PDA, which would have broken docs/05 §4's "the chain does not reveal project identity". Fixed by seeding the PDA `[r#const("olb_registry"), arg("project_tag")]`, which `spel` already supported. |
| 8 | "text fields diffed by line/word" | that is the **diff** rule; merge treats text as opaque | Auto-stitching two scientists' prose can fabricate an observation nobody wrote. |
| 9 | "Merges into `main` need N approvals" | direct non-merge commits to `main` are **allowed**; only merges are gated | Literal reading. Flag if you meant `main` to be push-protected too — it is a one-line change in `apply_commit`. |
| 10 | approvals, no binding stated | approvals sign `(mr_id, source_head, target_head)` and go stale when a head moves | Otherwise "approve, then push" defeats review completely. |
| 11 | "records added / edited / removed" | "removed" = a new version with `flags.retracted = true` | Records are append-only; there is no delete. |
| 12 | nothing on multiple merge bases | criss-cross is **not** merged recursively — report conflicts | Auditable over clever. A virtual base is content no one authored. |
| 13 | nothing on approver roles beyond "reviewers cannot approve" | eligibility evaluated at the **merge base** | Otherwise a membership change mid-review retroactively changes whether a merge was valid. |

Open, needs a decision before Phase 1b lands:

- **R10 is still open** (`docs/08`). This design assumes records cross the IPC
  seam as JSON strings, option 1. Nothing above depends on a typed wire format —
  `canon` is internal to `olb_core` and the Merkle leaves are raw bytes — so
  resolving R10 the other way does not invalidate this doc.
- **§0's shared-crate question.** `src = ./.` in `olb_core/flake.nix` makes a
  cross-directory path dep unbuildable. The committed-copy workaround is proven
  practice in this repo but should be confirmed rather than assumed.
