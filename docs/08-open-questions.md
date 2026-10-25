# 08 — Open questions and facts to verify

Answer these in Phase 0 from the Logos source, then edit this file (move answered items to "Resolved" with the evidence).

## Verify in the Logos source

Six of the original ten are answered from source (25 Oct 2026) and have moved to
"Resolved". What is left needs running code, not reading.

1. **Core module network access.** Can a core module make outbound HTTPS requests
   (needed for DOI lookups via Crossref/DataCite)? Nothing in the module contract
   forbids it — a core module is ordinary native code in its own process, not
   sandboxed like the QML view — but it is untested. If it works, `references.resolve`
   stands as specified. If not, drop the network lookup or route it through a
   user-pasted citation.
2. **File dialogs.** Can `olb_core` open a native file picker, or must `olb_ui`
   use a QML `FileDialog` and pass a path across IPC? The QML view is sandboxed to
   its own module directory, so a QML-side picker may not be able to reach a path
   outside it. Note `storage_module.uploadUrl(filePath, …)` takes an absolute path
   and `importFiles(path)` walks a directory, so core can certainly *read* arbitrary
   paths once it has one — the open question is only who chooses the path.
3. **LEZ from a core module.** How does `olb_core` submit an LEZ transaction?
   **This is the significant unknown.** Basecamp 0.3.0 pre-installs only
   `capability_module`, `modules_state`, `package_downloader`, `package_manager`
   and `package_manager_ui` (`BASECAMP_PREINSTALLED_MODULES` in scaffold
   `src/constants.rs`) — there is no wallet module, so there is no module-level
   route to the LEZ wallet. Candidate routes, in order of preference:
   a. speak the sequencer RPC directly from `olb_core` over HTTP (depends on Q1);
   b. ship a wallet-signing path inside `olb_core` using the LEZ account key;
   c. fall back to an out-of-process runner (`lgs wallet -- …`) for Phase 0 only
      and record the gap, as `docs/07` Phase 0 task 6 already allows.
4. **Delivery message size limit.** `send(contentTopic, payload)` takes raw bytes
   and base64-encodes them across the FFI boundary, but the ceiling is not
   documented in the module header. Measure it — it bounds how large a single
   commit or snapshot announcement can be before OLB has to chunk.

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

Answered on 25 October 2026 by reading the pinned source, not the published docs.
Clone paths are as cloned for that review; the repo and tag are what matter.

### R1 — Rust core module template (was Q1)
`logos-module-builder/0.3.0` provides the template as **`#rust`** — the source
directory is `templates/rust-module/`, but the flake attribute is `rust`
(confirmed by evaluating `.#templates`; the full set is `default`, `rust`,
`rust-with-external-lib`, `ui-qml`, `ui-qml-backend`, `with-external-lib`).
`docs/07` was right about this and an earlier revision of this file was wrong.
It is **Rust-first**: the trait
in `rust-lib/src/lib.rs` *is* the contract and the builder derives the `.lidl` from
it (`codegen.rust.trait`). No `build.rs`, no committed `.lidl`, no `Q_INVOKABLE`.
A Rust module **can** call C++ modules and vice versa, through generated typed
clients — `modules().<dep>.<method>(args) -> Result<T, E>` — proven end to end in
`doctests/cross-language-composition.test.yaml`. Dependencies are declared in
`metadata.json#dependencies` and as matching flake inputs.

### R2 — Return values and async (was Q2)
QML slot calls return a **pending reply**, delivered through
`logos.watch(pending, onValue, onError)`. There is no synchronous return on the
`ui-qml-backend` path. Rust→Rust and Rust→C++ calls through `modules()` are
synchronous and return `Result`; each also has a generated `<method>Async(args…,
callback)` overload.

**`logos.callModule` does not exist on this path.** It appears only as a
commented-out line in the QML-only `ui-qml` template and its skill — in no test
and no shipped module. The whole of `docs/06` was written against it. The real
chain is `QML --QtRO--> olb_ui C++ backend --LogosAPI--> olb_core`.

### R3 — Events to QML (was Q3)
Typed, pushed, no polling. A core module declares events (`logos_events:` in C++,
an `<Trait>Events` companion in Rust) and emits by calling them (`emit_<name>` in
Rust). The UI backend subscribes in `onContextReady()` via
`modules().<dep>.on<Event>(cb)` and feeds a `.rep` **PROP**; QtRO auto-syncs every
PROP change to the QML replica. Rich lists cross as a `QAbstractItemModel`
Q_PROPERTY on the backend, read in QML as `logos.model("<module>", "<prop>")`
(see `logos-chat-ui` `src/qml/ChatUi/ChatStore.qml`). Note the caveat recorded in
`logos-chat-ui/src/ChatBackend.rep`: model replicas do **not** proxy `Q_INVOKABLE`
lookups and a non-view caller cannot rely on `rowCount()`, so derived values must
be exposed as their own PROPs.

Full QML bridge surface: `logos.module()`, `logos.watch()`, `logos.model()`,
`logos.isViewModuleReady()`, `onViewModuleReadyChanged`.

### R4 — Storage module API (was Q4)
From `logos-storage-module/src/storage_module_plugin.h` (module version 3.0.2):

- upload: `uploadUrl(filePath, chunkSize, advertise)` → session id; CID arrives on
  the `storageUploadDone` event. Manual streaming via `uploadInit` / `uploadChunk`
  / `uploadFinalize`.
- download: `downloadToUrl(cid, filePath, local, chunkSize, isPrivate, advertise)`
  or `downloadChunks(…)` for base64 chunks over events.
- local presence: `exists(cid)` → bool. `manifests()` lists local manifests only.

**There is no pin primitive.** The closest is `fetch(cid, isPrivate, advertise)`
(caches locally in the background, emits nothing on completion) plus
`setAdvertise(cid, bool)` (announce to the DHT and serve to peers). Cached blocks
carry a TTL — `block-ttl`, default **30d** in the node config — so "pinned" in OLB
means *re-fetched on a schedule before the TTL expires*. `docs/06`'s `files.pin`
must be redefined in those terms.

**There is no network replica count.** Nothing reports how many nodes hold a CID.
The fallback named in the original Q4 is the only option: count acknowledgements
from member and replica nodes ourselves, over Delivery.

### R5 — Delivery module API (was Q5)
From `logos-delivery-module/src/delivery_module_plugin.h` (module version 0.3.3):

- `send(contentTopic, payload: bytes)` → request id; outcome via `messageQueued` /
  `messagePropagated` / `messageSent` / `messageError`.
- `subscribe(contentTopic)` / `unsubscribe(contentTopic)`.
- inbound: `messageReceived(messageHash, contentTopic, payload, source, timestamp)`.
- catch-up: `storeQuery(jsonQuery, peerAddr, timeoutMs)` — **requires an explicit
  store peer multiaddress** and is marked "⚠️ USE AT YOUR OWN RISK: backed by the
  kernel API … subject to change at any point without a deprecation cycle."
  Treat Store as a best-effort optimisation; snapshot-from-Storage is the real
  catch-up path (`docs/05` §5 should be reordered accordingly).

**RLN is not caller-configured.** `createNode` reads the network preset and brings
RLN up for deployments whose preset enables it; `rlnState()` reports
`Disabled | Initializing | Ready | Failed`. OLB manages no memberships. But
publishing **is** rate-limited by epoch quota — `messageQueued` fires when a send
is waiting for budget — which bounds how fast OLB can push a burst of commits.

**Also available and not in the handoff docs:** reliable SDS channels —
`channelCreate(channelId, contentTopic, senderId)`, `channelSend`, `channelClose`,
with channel state persisted across close. Potentially a better fit for project
commit sync than the raw bucket topics in `docs/01` §5. Evaluate in Phase 0.

### R6 — Module data directory (was Q9) — and it is a problem
`RustModuleContext` carries `instance_id` and `instance_persistence_path`, and
`LogosModuleContext` exposes `instancePersistencePath()` — a per-instance,
host-owned writable directory, empty when run outside a host.

**But it does not survive a relaunch.** `lgs basecamp launch <profile>` scrubs the
profile tree on every launch, explicitly including basecamp's `module_data/` and
`logs/`: "Everything under `module_root` lives inside the tree `launch` scrubs, so
basecamp's `module_data/` … and `logs/` … do not survive a relaunch. That is the
clean-slate contract working as intended — a module that needs state across
launches must not keep it there" (scaffold `docs/basecamp-module-requirements.md`).

`--user-dir` is **not** an escape hatch: it and `LOGOS_USER_DIR` are the same knob
`launch` itself sets per profile — "the same data-tree redirect reached from two
entry points, not independent mechanisms." So `CLAUDE.md` constraint 8 and the
Phase 1b acceptance test ("restarting with `--user-dir` keeps everything") do not
hold as written.

Decided 25 Oct 2026: **solve this in Phase 0**, before anything is built on top of
the assumption of durable local state. Options to test: a store location outside
the scrubbed tree, or a hand-launch path that bypasses `lgs basecamp launch`.

### R7 — Chat module persistence (was Q10)
Confirmed still ephemeral in 0.3.0. From `logos-chat-module/rust-lib/chat_module.lidl`:
"Each init is a new installation with a new address, so a conversation from a
previous session reads back with its messages and `history_only` set: it can be
labelled and deleted, but it cannot be sent into or invited to." Messages persist
as history; identity does not. `docs/04` §1 stands, and D2 (Delivery-only) is the
right call.

### R9 — Nix module builds fail on macOS 14.5 (open blocker, 25 Oct 2026)
**No Logos module builds on this machine**, including a stock `#rust` template
module. The failure is in nixpkgs' Qt wrapper hook, not in Logos code:
`logos-view-generator-0.1.0` compiles, links and installs, then its `fixupPhase`
segfaults (SIGSEGV, exit 139) inside
`wrap-qt6-apps-no-gui-hook/nix-support/setup-hook` line 80 — the
`find … -print0 | while read … isELF/isMachO …` loop. Everything downstream is
cascade.

Scope: `logos-view-generator` is a shared toolchain dependency, not UI-only, so
this blocks `olb_core` as much as `olb_ui`, and `lgs basecamp setup` with them.
Reproduced deterministically on a direct `nix-store --realise` of the single
derivation and again on a clean probe module.

Ruled out:
- *bash 5.3 dynamic-fd bug* — the `exec {fd}< … ; exec {fd}<&-` pattern those
  helpers use runs clean under the same `bash-5.3p3`.
- *Binary-cache workaround* — the builder flake declares no `nixConfig` and
  Logos publishes no substituter, so the derivation cannot be fetched prebuilt.

Leading hypothesis, unconfirmed: macOS 14.5 (Darwin 23.5.0, build 23F79) is
older than this nixpkgs/Qt combination is exercised against. The nixpkgs bash
reports a `darwin24.6.0` build triple, though `MACOSX_DEPLOYMENT_TARGET=11.3`
argues that alone should not matter. `wrapQtAppsHook` has a long history of
Darwin breakage (nixpkgs#102044, #174828) but no report matches this segfault.

Action: macOS update planned. Until then the Nix path is closed and work
proceeds in the pure-Rust `olb-records` crate (plain cargo, no Logos deps).
Re-test with a stock `#rust` template module after the update before assuming
it is fixed.

### R8 — Toolchain and packaging facts worth recording
- **Do not bump the `logos-module-builder` 0.3.0 pin.** It locks exactly the
  `logos-protocol` / `logos-cpp-sdk` revs basecamp 0.3.0 speaks; 0.3.1 is already
  ahead, and a mismatched module may install cleanly but have its calls refused
  at load time. Tags 0.3.1–0.3.3 exist; none is for us.
- **Module flakes must be immediate children of the project root.** Auto-discovery
  reads the root flake, else exactly one directory level down
  (`resolve_install_sources` in scaffold `src/commands/basecamp.rs`), skipping
  `target`, `node_modules`, `result` and dotted dirs. `modules/olb_ui` would never
  be found. `CLAUDE.md`'s target layout is corrected accordingly.
- **Sibling sub-flakes** must use the single-line `<name>.url = "path:../<sibling>"`
  form — the flake.nix parser is line-level and misses nested attrset forms.
- **Every sub-flake needs** `<dep>.inputs.logos-module-builder.follows =
  "logos-module-builder"`, or a second module-builder node enters `flake.lock` and
  silently wins under scaffold, producing opaque build failures.
- **A `ui_qml` package must ship a 256×256 PNG icon** (bundled to
  `assets/icon.png`), and `main` / `view` in `metadata.json` must point at files
  that exist in the built variant — validation rejects a manifest naming a missing
  entry point.
- **macOS socket-path budget:** `sockaddr_un.sun_path` is capped at 104 bytes and
  the per-profile runtime root plus `logos_token_<module>` overflows it for a
  typical project path. `launch` defaults `runtime_dir` to `/tmp/lgs-<profile>` on
  macOS, which fits; keep any override short.
- **Custom profiles are fine.** `basecamp setup` seeds only `alice` and `bob`, but
  "any declared profile launches; unknown profiles are seeded on first launch" —
  so `carol`, `dave` and `replica` work via `[basecamp.profiles.<name>]`.
- **`lgs new programs` does not exist.** LEZ projects come from
  `lgs create --template lez-framework`; guest programs live in
  `methods/guest/src/bin/*.rs` and deploy with `lgs deploy`. `docs/07` task 6 is
  corrected.
