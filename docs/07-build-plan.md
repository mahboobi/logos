# 07 — Build plan

Phases are ordered so a real lab can use the notebook as early as possible. Each phase ends with an acceptance test run on scaffold profiles. Estimates assume one developer working with Claude Code; treat them as rough until Phase 0 is done.

| Phase | Scope | Rough effort |
| --- | --- | --- |
| 0 | Spike: toolchain, two modules talking, one record synced alice ↔ bob, file stored/fetched, hash anchored on localnet | 1–2 weeks |
| 1a | UI shell + design system + all screens on fixtures | 2–3 weeks |
| 1b | Identity, projects, records, VCS, sync, files, replica, anchoring (live data for S1, S2, S6, S8) | 4–6 weeks |
| 1c | Members and invites (S7, M1) incl. key rotation | 2–3 weeks |
| 2 | Hypotheses commit/reveal (S3), publishing (S5), merge requests | 4 weeks |
| 3 | Funding escrow program + S4, credit program | 5–6 weeks |
| 4 | Labbook agent suggestions, security review, catalogue release | 4 weeks |

---

## Phase 0 — Spike

**Tasks**
1. Install Nix (flakes), Rust ≥ 1.81, `lgs` at `d4b15ff`. Run `lgs basecamp setup`.
2. Scaffold `olb_ui` from `logos-module-builder/0.3.0#ui-qml-backend` and `olb_core` from **`#rust-module`** (not `#rust` — that template does not exist). Both as **immediate children of the repo root**, not under `modules/`: scaffold discovers module flakes exactly one level down. Pin the builder in each `flake.nix`, and add `<dep>.inputs.logos-module-builder.follows = "logos-module-builder"` to every sub-flake.
3. `olb_core.session.status` returns a JSON string; `olb_ui` shows it.
4. `olb_core` publishes a signed test record on a Delivery topic; second profile receives and displays it.
5. Upload a file through `storage_module`, fetch it on the other profile by CID.
6. LEZ programs: there is no `lgs new programs` command — use `lgs create --template lez-framework` (or lift its layout into this repo). Write `olb_registry.anchor(tag, root, count)` as a guest in `methods/guest/src/bin/olb_registry.rs`, deploy to localnet with `lgs deploy` (`RISC0_DEV_MODE=1`), and call it from a host runner in `src/bin/`. **Calling it from `olb_core` is an open risk**: Basecamp 0.3.0 ships no wallet module, so there is no module-level route to the LEZ wallet. Record which route works (`docs/08` item 3).
7. **Persistence.** `lgs basecamp launch` scrubs `module_data/` on every launch, so `instancePersistencePath()` does not survive a relaunch and `--user-dir` is the same knob, not a workaround (`docs/08` R6). Find and prove a store location for identity, keys and SQLite that survives a relaunch, before Phase 1b builds on it.
8. Measure the Delivery `send` payload ceiling; it bounds commit and snapshot size.
9. Evaluate Delivery's reliable SDS channels (`channelCreate`/`channelSend`) against the bucket-topic design in `docs/01` §5 before committing to either.
10. Write findings into `docs/08-open-questions.md` (answer each question you can).

**Acceptance:** with `lgs basecamp launch alice` and `lgs basecamp launch bob`, a record typed on alice appears on bob within 10 s; a 5 MB file uploaded on alice downloads on bob with a matching SHA-256; an anchor transaction for its hash is visible on the local sequencer.

**Prompt**
```text
Phase 0. Follow docs/07 Phase 0 tasks 1–7. Verify every Logos call against the
module API references before using it. Stop after task 2 and show me the
generated layout and the exact methods the templates expose. At the end, run the
acceptance test on alice and bob and paste the evidence. Update docs/08 with
what you learned.
```

## Phase 1a — UI on fixtures

**Tasks**
1. Add `design/Theme.qml` as a singleton; bundle IBM Plex Sans/Mono with `FontLoader`.
2. Build components from `docs/02-design-system.md` §4 with a gallery page (dev only).
3. Build S1–S8 and M1 against `OLB_FIXTURES=1`, matching `design/canvas/*.dc.html` copy and spacing.
4. Navigation between screens; modal open/close; filter pills; New Project live preview; invite modal steps.
5. Keyboard and screen-reader pass (`docs/02` §6).

**Acceptance:** every screen in `docs/03-screens.md` renders with fixture data at 1440 px and at 960 px window width; all interactions listed per screen work; a side-by-side check against the canvas PNG exports shows no missing sections.

**Prompt**
```text
Phase 1a. Build the QML UI against fixtures. Start with Theme.qml, fonts and the
component gallery; show me screenshots of the gallery before building screens.
Then build screens in this order: S1 Board, S2 Entry, S8 New project, S7 Members
+ M1 invite modal, S6 History, S3, S4, S5. Match docs/03 and the .dc.html files.
```

## Phase 1b — Live core

**Tasks**
1. Identity generation, storage, `identity.me` / `identity.parse`, OLB ID format and fingerprint (`docs/04` §2).
2. Local store (SQLite) for records, commits, refs, keys (encrypted at rest).
3. Record model + canonical JSON + signing (`docs/05` §1–2).
4. Commit DAG, branches, tags, restore, diff (`docs/05` §3); unit tests.
5. Project + idea creation from S8 payload; privacy modes; idea keys.
6. Sync over Delivery bucket topics; head exchange; snapshots to Storage (`docs/05` §5).
7. Files: encrypt, upload, pin on all members; copy counts; `olb_replica` module and `replica` profile.
8. Anchoring queue + Merkle proofs + `olb_registry` on localnet, then testnet.
9. Wire S1, S2, S6, S8 to live data; keep fixtures mode working.

**Acceptance:** alice creates a Private project (S8), adds an idea and Run entry with a file; bob (pre-provisioned member for this phase) sees it live; replica shows the file pinned (copies ≥ 3); history shows commits with "Batch n" after anchoring; "Verify proof" succeeds; killing alice and relaunching keeps everything — **via whatever persistence route Phase 0 established, not `--user-dir`**, which is the same knob `lgs basecamp launch` already sets and scrubs (`docs/08` R6).

## Phase 1c — Members and invites

**Tasks:** inbox topics, InviteEnvelope, invite by ID, invite codes and join requests, accept/decline on invitee side, pending invite states, role/access enforcement on incoming commits, removal + key rotation, S7 and M1 live.

**Acceptance:** alice invites carol by OLB ID as Reviewer with access to 2 of 5 ideas; carol accepts; carol can read only those ideas and cannot commit edits (bob's node rejects a forged edit from carol); alice invites dave by code, dave redeems, alice approves after comparing fingerprints; alice removes dave → key version increments and dave cannot decrypt a new entry.

## Phase 2 — Hypotheses and publishing

Commit/reveal with `olb_registry.commit`, review approvals, auto-tags, merge requests UI, publish bundle (RO-Crate export, reveal verification, public CIDs, open review topic), credit table.

**Acceptance:** pre-register on alice, add data after, publish; an outside profile (no membership) verifies the reveal matches the earlier commitment and downloads the public bundle.

## Phase 3 — Funding and credit

`olb_funding` (escrow, public and private contributions, milestone approval threshold enforced in program, release to project shared account), `olb_credit`, S4 live.

**Acceptance:** three reviewer profiles; release of M2 happens only after the second approval; a contributor profile funds privately and their identity does not appear in program state.

## Phase 4 — Agent and hardening

Labbook agent as separate key (operator-signed), suggestions API, external security review of identity/invite/crypto, portable `.lgx` builds (`lgs basecamp build --variant lgx-portable`), catalogue submission.

---

## Test plan (all phases)

- **Unit:** canonical JSON, signatures, Merkle proofs, commit DAG merge base, conflict detection, KEM seal/unseal, key rotation, OLB ID parse/verify, invite state machine.
- **Integration:** scaffold profiles `alice` (lead), `bob` (contributor), `carol` (reviewer), `dave` (newcomer), `replica`. Add them to `scaffold.toml` under `[basecamp.profiles.<name>]`.
- **Resilience:** offline member catches up from snapshot; Delivery message dropped → resend works; Basecamp restart (with `--user-dir`) keeps identity, keys and data.
- **UI:** fixture snapshots per screen; keyboard-only run through M1; contrast check of any new colour.
