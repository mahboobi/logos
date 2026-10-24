# CLAUDE.md — Open Labbook (Basecamp app)

Standing instructions for every Claude Code session in this repository. Detailed specs live in `docs/`. When a doc and the real Logos source disagree, the source wins: say so, then update the doc in the same change.

## What we are building

Open Labbook (OLB) is a desktop app that runs inside **Logos Basecamp**. Research groups use it to move work through six stages — Idea, Hypothesis, Funding, Experiments, Data/Results, Publishing — with every record signed, versioned (git-like), shared peer to peer, and anchored on the Logos Execution Zone (LEZ). Read `docs/01-product-and-architecture.md` first.

## Pinned stack (do not upgrade without asking)

| Component | Pin | Notes |
| --- | --- | --- |
| Logos Basecamp | 0.3.0 | Testnet v0.3 |
| logos-scaffold (`lgs`) | 0.4.0, commit `d4b15ff` | `cargo install --locked --git https://github.com/logos-co/scaffold --rev d4b15ff logos-scaffold` |
| logos-module-builder | 0.3.0 | Pin it in every `flake.nix`; an unpinned URL resolves to the newest release |
| LEZ (sequencer, wallet) | v0.1.2 | Set by scaffold |
| spel | v0.5.0 | Set by scaffold |
| RISC Zero guest builder | r0.1.97.0 | Set by scaffold |
| Nix | flakes enabled | Required for every build |

Development runs on Linux (x86_64/aarch64) or macOS only. Basecamp setup needs ~5 GB free memory on first run.

## Repository layout (target)

```
olb-basecamp/
  CLAUDE.md
  scaffold.toml                 # lgs project file (committed)
  modules/
    olb_ui/                     # UI module, ui-qml-backend template, QML
    olb_core/                   # core module, rust template
    olb_replica/                # headless core module for always-on replica nodes (Phase 1b)
  programs/                     # LEZ guest programs (logos-scaffold LEZ project)
    olb_registry/  olb_funding/  olb_credit/
  design/  docs/  fixtures/     # from this handoff package
```

Confirm the layout with the user before scaffolding (kickoff step 3).

## Hard constraints from the platform

1. **QML UI modules are sandboxed.** No network, no file access outside the module directory. Every network, storage, crypto or chain operation goes through a core module: `logos.callModule("olb_core", "<method>", [args])`. Never try to work around the sandbox.
2. **Each module runs in its own process.** Calls between modules are IPC. Keep payloads as JSON strings; keep them small; never pass file contents through the UI — pass CIDs and paths.
3. **Logos Storage has no paid persistence.** A file lives only while some node holds it. OLB must pin every project file on every member node plus at least one replica node, and show copy counts in the UI.
4. **Delivery Store is temporary.** Sync must survive by snapshotting project state to Storage, not by relying on message history.
5. **Logos Chat addresses change every time Basecamp starts** (testnet). Never treat a Chat address as an identity. Identity = OLB signing key + ML-KEM-768 sealing key (`docs/04-invite-flow.md`).
6. **Chat groups cannot remove members.** Do not use Chat groups for project membership.
7. **LEZ shared private accounts are 1-of-n at the key layer.** Any spending threshold (for example "2 of 3 reviewers") must be enforced inside the LEZ program.
8. **`lgs basecamp launch <profile>` wipes the profile.** Use it for clean tests. For state that must survive restarts, run Basecamp with `--user-dir`.
9. **Nix package builds only see git-tracked files.** `git add` every new file before `nix build .#lgx` or `lgs basecamp build`.

## UI rules

- Follow `docs/02-design-system.md` and `docs/03-screens.md` exactly. Use `design/Theme.qml` tokens; never hard-code a colour or size that has a token.
- Colour carries meaning: green = verified/done/anchored, violet = current/committed/in progress, amber = needs attention, slate = neutral/draft. Do not use them decoratively.
- Minimum touch target 44 px. Every control is keyboard reachable and has an accessible name. Text contrast at least 4.5:1.
- Copy is final unless marked otherwise. Keep sentence case. No emoji.
- Build each screen against `fixtures/sample-project.json` first (a fixture mode in `olb_core` or a QML mock), then wire to live data.

## Engineering rules

- Rust for `olb_core` and LEZ programs; share record types between them in one crate where the toolchains allow.
- Records are append-only. An edit creates a new version and a new commit. Nothing in history is ever rewritten.
- Every user-visible action that changes data produces a signed commit (`docs/05-version-control.md`).
- Crypto: use audited crates only (ed25519-dalek, an ML-KEM-768 implementation, chacha20poly1305 or aes-gcm, sha2, hkdf). No home-made primitives.
- Secrets never leave `olb_core`. The UI never sees private keys, project keys or salts.
- Write tests for: record versioning, commit DAG and merge, Merkle proofs, invite sealing/unsealing, key rotation.
- Use `RISC0_DEV_MODE=1` for local LEZ iteration; never ship dev-mode proofs to testnet.

## Testing peer-to-peer features

Use scaffold profiles: `lgs basecamp launch alice` in one terminal, `lgs basecamp launch bob` in another. Add `carol` (reviewer) and `replica` profiles in `scaffold.toml` when testing invites and replication. Every phase's acceptance test is written in terms of these profiles (`docs/07-build-plan.md`).

## Do not

- Do not invent Logos APIs. Read the module API references first:
  - Storage: https://logos-co.github.io/logos-storage-module/latest/api_reference.html
  - Delivery: https://logos-co.github.io/logos-delivery-module/latest/pages/api_reference.html
  - Chat: https://logos-co.github.io/logos-chat-module/latest/pages/api_reference.html
  - Core modules: https://docs.logos.co/core/build-modules/build-and-run-a-logos-core-module
  - Scaffold: https://docs.logos.co/build-an-app/scaffold/about-logos-scaffold
  - LEZ programs: https://docs.logos.co/lez/programs/write-and-deploy-lez-program-with-scaffold
- Do not add a token (UBR or any other) in this codebase. Funding uses LEZ testnet tokens only.
- Do not store human-subject or clinical data in fixtures or tests.
- Do not change pins, privacy semantics or the record format without asking.
- Do not mark a phase done until its acceptance test passes on two profiles.
