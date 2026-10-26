# 05 — Records and version control

OLB keeps a git-like history per project: content-addressed records, a commit DAG, branches, merge requests, tags, diffs and restore. Commits are anchored on LEZ in batches.

## 1. Research object (record)

```json
{
  "id": "olb:obj:<sha256 of first version, base32>",
  "type": "project | idea | hypothesis | proposal | entry | protocol | dataset | result | publication | review | member",
  "version": 3,
  "project": "olb:proj:<id>",
  "idea": "olb:obj:<idea id> | null",
  "authors": [{ "key": "<ed25519 pk>", "role": "CRediT role", "orcid": null }],
  "agents": [{ "key": "<agent pk>", "model": "name@version", "operator": "<human pk>" }],
  "parents": ["olb:obj:…"],
  "fields": { "…": "type-specific, see §1.1" },
  "attachments": [{ "name": "plate3_meltcurves.csv", "cid": "logos:…", "sha256": "…", "size": 421888, "encrypted": true }],
  "flags": { "negative_result": false },
  "visibility": "private | group | public",
  "created_at": "RFC 3339"
}
```

Stored encrypted with the idea key (or project key for project-level types) unless the project is Open or the record is published. Canonical JSON (RFC 8785) is used for every hash and signature.

### 1.1 Type-specific fields (minimum)

| Type | Fields |
| --- | --- |
| idea | `title`, `summary`, `question`, `motivation`, `stages_enabled[]`, `current_stage` |
| hypothesis | `label` (H1), `statement`, `predictions[] {label, text}`, `falsification`, `analysis_plan`, `review {approvals[]}`, `commitment {fingerprint, batch} | null`, `revealed` |
| proposal | `title`, `budget`, `milestones[] {id, title, amount, evidence[], state}`, `reviewers[]`, `threshold`, `lez_program_account` |
| entry | `title`, `purpose`, `protocol_ref`, `parameters[] {name, value, unit, source}`, `observations`, `deviations`, `status: draft | shared` |
| dataset | `title`, `ro_crate_cid`, `files[]`, `derived_from[]` |
| result | `statement`, `hypothesis_ref`, `outcome: held | not_held | partial`, `notes` |
| publication | `title`, `manuscript_cid`, `licence`, `bundle_cid`, `reveals[]`, `credits[]` |

## 2. Commit

```json
{
  "id": "<sha256 of canonical commit without sig, hex; short form = first 7 chars>",
  "project": "olb:proj:<id>",
  "parents": ["<commit id>"],
  "branch": "main",
  "author": "<ed25519 pk>",
  "co_signers": ["<agent pk or approver pk>"],
  "message": "Run 14: add plate 3 melt curves",
  "changes": [
    { "object": "olb:obj:…", "from_version": 2, "to_version": 3, "kind": "edit | create | flag | restore" }
  ],
  "files_added": ["logos:…"], "files_removed": [],
  "timestamp": "RFC 3339",
  "sig": "<ed25519 signature>",
  "co_sigs": ["…"]
}
```

- Every user action that changes data produces exactly one commit (batch edits from a save = one commit).
- Merge commits have two parents and list approvers in `co_signers`.
- Commits travel on the project sync topic, encrypted with the project key.

## 3. Branches, merges, tags

| Concept | Behaviour |
| --- | --- |
| Branch | Named ref → commit id. `main` always exists and is protected. Branches are per project; a branch usually touches one idea. |
| Protection | Merges into `main` need N approvals (project setting: 0/1/2). Reviewers cannot approve merges. |
| Merge request | `{id, source, target, title, author, reviewers[], approvals[], state: open | merged | closed}`; diff summary = records added / edited / removed. |
| Conflict | Two branches changed the same field of the same record since the merge base. Resolve field by field in a merge view (to design later); never auto-merge conflicting fields. |
| Tag | Name → commit id, immutable. Auto-tags (if enabled): `h{n}-preregistered`, `m{n}-released`, `preprint-v{n}`, `protocol-v{n}`. |
| Restore | Creates a new commit whose changes set a record back to an older version. History is never rewritten. |
| Diff | Field-level: text fields diffed by line/word; tables by row; files as added/removed/changed. |

## 4. Anchoring on LEZ

- Queue commit ids locally. On flush, build a Merkle tree (SHA-256, sorted pair hashing) and call `olb_registry.anchor(project_tag, root, count)`.
  - **Implemented 26 Oct 2026.** The registry account is a PDA seeded
    `[const("olb_registry"), arg("project_tag")]`, so each project gets its own
    account. The first cut used `literal("olb_registry")` alone, which resolved
    to ONE account for every project on every node — concurrent anchors from
    two members silently overwrote each other. Changing it mints a new
    `program_id`, so anchors made before this change are orphaned; only the one
    test anchor existed, so nothing real was lost.
  - Note docs/10-vcs-design.md specifies RFC 6962 hashing (prefixed leaves,
    no last-leaf duplication) rather than sorted pair hashing, because sorted
    pairs allow a second-preimage forgery. Where the two disagree, docs/10
    wins and this line should be updated when the tree is implemented.
- Store each commit's Merkle path with the batch number. "Verify proof" recomputes the root and checks it against the program account.
- Flush policy from project settings: every change / every few minutes (default; target 5 min or 100 commits) / daily / key moments only (pre-registration, milestone, publication — always flush immediately at these).
- `project_tag` = `H(project_id || project_salt)` for Private/Invite-only projects so the chain does not reveal project identity.
- Commitments (hypothesis, idea) are separate calls: `olb_registry.commit(tag, fingerprint)`; reveal = publishing salt + record; verification recomputes the fingerprint.

## 5. Sync and snapshots

- Head exchange: on start and every 60 s, each node publishes its branch heads; peers request missing commits by id.
- Snapshot every 50 commits or daily: encrypted dump of all current record versions + refs to Storage; CID announced on the sync topic; replica pins it. New members bootstrap from the latest snapshot plus subsequent commits.
- Validation on receipt: signature, author is a member with the right role, parents known (else fetch), change versions consistent. Invalid commits are dropped and logged, never applied.

## 6. Graph data for the UI

`vcs.log` returns rows with precomputed layout so QML only draws:

```json
{ "id": "e19f5a0", "message": "Merge alt-buffer: switch DSF buffer to HEPES",
  "lane": 0, "lanes_active": [0, 1], "edges": [{"from_lane": 1, "to_lane": 0, "kind": "merge"}],
  "is_merge": true, "badges": [{"kind": "merge", "text": "Merge · reviewed by 2"}],
  "meta": "Priya Raman · approved by Arjun Mehta, Lin Wei · 2 days ago · batch 205",
  "anchor": {"state": "anchored", "batch": 205} }
```
