# 01 — Product and architecture

## 1. What Open Labbook is

Open Labbook (OLB) is the electronic lab notebook of Unblock Research, rebuilt as a local-first, peer-to-peer app on Logos Basecamp. A research group creates a **project**; inside it, each line of inquiry is an **idea** that moves through up to six **stages**:

| # | Stage | What the group produces | Key property |
| --- | --- | --- | --- |
| 1 | Idea | Idea note: question, motivation, links | Timestamped privately (salted hash on LEZ) |
| 2 | Hypothesis | Pre-registration: hypothesis, predictions, falsification criteria, analysis plan | Committed before data exists; revealed at publication |
| 3 | Funding | Proposal, budget, milestones; escrow | Funds released only on reviewer-approved evidence |
| 4 | Experiments | Notebook entries: protocol, parameters, observations, deviations, files; negative results | Signed, versioned, synced live |
| 5 | Data and results | Datasets (RO-Crate manifest), analysis outputs, "prediction held / not held" | Encrypted until release; pinned on replicas |
| 6 | Publishing | Bundle: manuscript + data + code + provenance chain; credit records | Public; reveals earlier commitments |

Projects choose which stages they use (New Project screen). Negative results are first-class records, never hidden.

Everything a user creates is a **research object** (`docs/05-version-control.md` has the record format). Every change is a **signed commit**. Commits are **anchored** on LEZ in batches so outsiders can verify when something existed.

### Users and roles

| Role | Can do |
| --- | --- |
| Lead (owner) | Everything: write, approve merges, commit hypotheses, publish, invite and remove members |
| Contributor | Read private records; write entries; open branches; approve merges into main; propose hypothesis commits |
| Reviewer | Read the ideas they are given; approve funding milestones; cannot edit records |
| Labbook agent (AI) | Proposes edits under its own key, operated by a human; nothing applies until a human accepts |

Researchers never approve funding for their own work.

### Privacy modes (chosen at project creation, immutable)

| Mode | Who can read | What goes on-chain |
| --- | --- | --- |
| Open | Anyone | Content published unencrypted to Storage; commits anchored openly |
| Private | Members only | Only salted fingerprints; content encrypted with the project key |
| Invite only | Members only, but title/summary/field are listed publicly so others can ask to join | Listing public; records as Private |

## 2. Logos building blocks used

| Logos component | What OLB uses it for | Constraint to respect |
| --- | --- | --- |
| Basecamp runtime | Hosts the app; loads modules; each module in its own process | Pinned to 0.3.0 |
| UI module (QML) | All screens | Sandboxed: no network or files; talks only to `olb_core` |
| Core module (Rust) `olb_core` | All logic: records, crypto, sync, VCS, anchoring, invites | Only component allowed to call other Logos modules |
| Logos Storage (`storage_module`) | Files, snapshots, published bundles (CID-addressed) | Organic replication only; OLB must pin |
| Logos Delivery (`delivery_module`) | Live sync of commits within a project; invite inboxes | Store is temporary; topic names visible to Store/Filter peers |
| Logos Chat (`chat_module`) | Optional human messaging; not used for identity or membership | Address changes per launch; groups cannot remove members |
| LEZ programs | `olb_registry` (anchors, commitments), `olb_funding` (escrow, milestones), `olb_credit` (contribution records) | Risc0 zkVM, Rust; tooling docs are early drafts |
| LEZ private and shared accounts | Private contributions; project funding account | Shared accounts are 1-of-n; thresholds in program |

## 3. Components

```
┌──────────────────────── Researcher's Basecamp ────────────────────────┐
│  olb_ui (QML)  ── logos.callModule() ──►  olb_core (Rust core module)  │
│                                            │  records, VCS, crypto,    │
│                                            │  sync, anchoring, invites │
│        ┌───────────────┬───────────────────┼──────────────┬─────────┐ │
│   storage_module  delivery_module      chat_module     LEZ wallet    │
└────────┬───────────────┬───────────────────────────────────┬─────────┘
         │ files/CIDs    │ signed, encrypted commits          │ txs
   other members'   ◄────┴──► other members' olb_core    LEZ programs
   Storage nodes          replica node (olb_replica)      olb_registry
                                                          olb_funding
                                                          olb_credit
```

| Component | Type | Responsibility |
| --- | --- | --- |
| `olb_ui` | UI module, QML (`ui-qml-backend` template) | All screens in `docs/03-screens.md`; no logic beyond presentation and form state |
| `olb_core` | Core module, Rust (`rust` template) | Local store (SQLite in module data dir), record model, commit DAG, signing, encryption, Delivery sync, Storage pinning, anchoring queue, invites, membership, fixtures mode |
| `olb_replica` | Core module on a headless Logos Node | Joins projects as a non-reading member; pins every CID; serves files; cannot read or sign records |
| `olb_registry` | LEZ program | Batch anchors (Merkle roots), salted commitments, reveal checks |
| `olb_funding` | LEZ program | Proposal escrow, public/private contributions, milestone release after M-of-N reviewer approval |
| `olb_credit` | LEZ program | Contribution records per object and role; later feeds URID reputation |

## 4. Data flows

### Writing a notebook entry
1. User edits in `olb_ui`; the UI calls `olb_core.record.saveDraft` on debounce.
2. "Sign and share" calls `olb_core.record.commit`: core creates a new record version, a commit, signs both, encrypts the payload with the idea key, stores attachments in Storage (encrypted), and publishes the commit on the project's Delivery topic.
3. Other members' `olb_core` receive, verify signature and parent, apply, pin attachments, emit `recordChanged` to their UI.
4. The anchoring queue adds the commit id to the next batch; on flush, core sends the Merkle root to `olb_registry` and stores each commit's proof.

### Sync and recovery
- Live: Delivery content topic per project bucket (see §5).
- Catch-up: on start, core asks peers for missing commits by head hash; if none online, loads the latest project snapshot CID (written to Storage every N commits and pinned by the replica).

### Files
- Upload: encrypt (if not Open) → Storage upload → CID + SHA-256 stored in the record.
- Pinning: every member node and the replica fetch every project CID; the UI shows a copy count (`files.status`).

## 5. Privacy details that affect implementation

- **Content topics leak.** Use a small number of shared bucket topics (`/olb/1/sync-<bucket>/proto` where `bucket = H(projectId) mod 16`), not one topic per project, so topic names do not reveal which labs collaborate. Payloads are encrypted and carry the project id inside the ciphertext.
- **Idea-level keys.** Each idea has its own symmetric key, wrapped by the project key. This lets a Reviewer be given access to selected ideas only (Invite step 2).
- **Salted commitments.** Hypothesis and idea commitments are `SHA-256(salt || canonical-json(record))`; the salt stays private until reveal.
- **No personal data on-chain.** Only hashes, Merkle roots, program state and token movements.

## 6. Relationship to the Unblock whitepaper

OLB on Logos is the pilot of the whitepaper's Open Labbook dapp. Contracts are kept small and chain-portable so the work survives the sovereign-vs-rollup decision gate. No UBR token logic belongs in this codebase.
