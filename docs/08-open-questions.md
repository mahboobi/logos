# 08 — Open questions and facts to verify

Answer these in Phase 0 from the Logos source, then edit this file (move answered items to "Resolved" with the evidence).

## Verify in the Logos source

1. **Rust core module template.** What does `logos-module-builder/0.3.0#rust` generate? How are methods exposed (Qt `Q_INVOKABLE` via a C++ shim, or a Rust macro)? Can a Rust module call other modules (`storage_module`, `delivery_module`) through `LogosAPI`?
2. **Return values and async.** Does `logos.callModule` in QML return a value synchronously, a promise, or require a callback? What is the payload type limit?
3. **Events to QML.** How does a core module emit events that a QML UI module receives (Qt signals over Remote Objects, a subscription call)? See the `ui-qml-backend` template.
4. **Storage module API.** Exact methods for upload, download, local presence check and "keep this CID" (pin). Is there a way to count network replicas, or must OLB count acknowledgements from member nodes itself? (Default plan: count member/replica acknowledgements.)
5. **Delivery module API.** Subscribe/publish signatures, content topic format, Store queries for missed messages, message size limit, whether RLN membership is required to publish on testnet.
6. **Core module network access.** Can a core module make HTTPS requests (needed for DOI lookups)? If not, drop `references.resolve` network lookups or route them differently.
7. **File dialogs.** Can a core module open a native file picker, or must QML use a `FileDialog` and pass a path (allowed by the sandbox?)
8. **LEZ from a core module.** How does a core module submit an LEZ transaction (wallet module API? call the LEZ wallet CLI?). Which Basecamp module exposes the wallet?
9. **Module data directory.** Where does a module persist state (`module_data/`)? Is it per profile? Is it backed up anywhere?
10. **Chat module persistence.** Confirm that 0.3.0 still regenerates the Chat identity on every launch (docs say so for testnet v0.2.x).

## Product decisions still open

| # | Decision | Default in these docs | Owner |
| --- | --- | --- | --- |
| D1 | Own ML-KEM sealing key vs LEZ wallet sealing key | Own key | Mahboob / Gautham |
| D2 | Use Logos Chat at all (vs Delivery-only) | Delivery-only for invites and sync; Chat optional later for human messaging | Mahboob |
| D3 | Per-project Zone channel vs LEZ program for anchoring | LEZ `olb_registry`; revisit when Zone SDK is stable | Gautham |
| D4 | Snapshot cadence | Every 50 commits or daily | Engineering |
| D5 | Default invite expiry | 14 days (ID), 48 h (code) | Mahboob |
| D6 | Dark mode | Not in scope | Design |
| D7 | Windows support for end users | Unknown — Basecamp end-user builds to be checked; dev tooling is Linux/macOS only | Mahboob |
| D8 | Profile / "Copy my ID" screen | Simple panel from the nav status box, to be designed | Design |
| D9 | Merge conflict resolution UI | Field-by-field view, to be designed in Phase 2 | Design |
| D10 | Relationship to UR Chain / UBR | None in this codebase; contracts kept portable | Founders |

## Known design gaps (build a reasonable version, flag for review)

- Empty states for a brand-new project (no ideas yet).
- "Manage member" sheet (change role, change idea access, remove).
- Invitee side: "Join {project}?" prompt and "Join a project" (paste code) screen.
- Profile screen with "Copy my ID", fingerprint, backup/export of keys.
- Merge request review screen and conflict resolution.
- Error and offline states beyond the inline amber panel.

## Resolved

(Move items here with a link to the source file or doc page that answered them.)
