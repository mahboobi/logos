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
| D11 | Basemap tiles for the Experiments maps: can `olb_core` fetch OSM tiles, or bundle offline vector tiles for the study area? | Placeholder background until decided; prefer offline for privacy | Engineering |
| D12 | Field capture on phones — Basecamp is desktop only | Desktop flow (photo EXIF prefill) for now | Mahboob |
| D13 | Species list source: GBIF needs network, project list is offline | Project list; GBIF optional later | Mahboob |
| D14 | Media replication policy change for photo-heavy protocols | **Decided 27 Oct 2026: accept, as a per-project setting.** CLAUDE.md rule 3 amended | Mahboob |
| D15 | Visitor spam: is testnet RLN enough? | Visitors off by default, behind a flag | Engineering |
| D16 | Can a Reviewer-role member verify submissions? | No by default; project setting | Mahboob |
| D17 | **Paid access to project data — unresolved, see below** | None; blocks `docs/12` §2.3 and §6 | Mahboob |

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

### R6 — Durable state across a relaunch (PARTLY RESOLVED 26 Oct 2026)
`lgs basecamp launch` scrubs the whole profile tree on every launch, so
`instancePersistencePath()` cannot hold anything that must outlive it. The
store therefore lives **outside** that tree:

```
~/Library/Application Support/OpenLabbook/<LOGOS_PROFILE>/store.json
```

`LOGOS_PROFILE` is exported per profile by `launch`, so alice and bob stay
isolated. Writes go to a temp file and are renamed, so a crash mid-write
cannot truncate the store. Projects, notes, the profile and the signed commit
log all survive a relaunch now.

Deliberately a JSON file, not SQLite: Phase 0 needs durability and an
auditable append-only commit log, not query power. The same shapes can move
into SQLite in Phase 1b without changing the module API.

**Still open:** the ed25519 signing key is NOT yet persisted — it is
regenerated on every launch, so a node's identity (and therefore the author on
every commit) changes between runs. Settings says so on screen. Durable
identity needs the key sealed at rest, which is a key-management decision, not
just a file write.

### D17 — "Those who pay to access storage" (RAISED 27 Oct 2026, BLOCKING parts of docs/12)

Stated in review of the data-collection handoff: *"Data is stored in Logos
storage and accessible only to members of the project and those who pay to
access storage."*

That sentence does not fit the model the code and docs currently implement,
and it cannot be built from a one-line description, so nothing has been
written against it. Three readings, with different consequences:

1. **Storage cost, not access control.** Someone pays a node to *host* bytes
   they are already entitled to read. This changes nothing about who can
   decrypt, and is close to what CLAUDE.md already says about Storage having
   no paid persistence. Cheapest to build; probably not what was meant,
   since it would not need saying.
2. **Paid read access to project data.** A non-member pays and can then
   decrypt project records. This is a product and a crypto change: it needs
   a payment rail, and it needs content keys to reach someone who is not a
   member, which the hybrid key model (docs/04 §6) deliberately does not
   allow. It also collides with CLAUDE.md's "do not add a token" rule —
   LEZ testnet tokens are the only permitted value, and nothing in the
   current design gates decryption on payment.
3. **Paid access to the public tier only.** The `public` half of a
   submission (docs/12 §2.3) is served to payers; `restricted` stays
   members-only. Narrower, but it contradicts the word *public* and would
   remove the open-science story docs/12 §6 builds visitors on.

**Why it blocks.** docs/12 §2.3 splits every submission into `public` and
`restricted` and §6 lets visitors submit to Open projects. Reading 2 or 3
would change what `public` means, who holds which key, and whether an Open
project is open at all. Guessing would mean writing the privacy model twice.

Not blocking: integration of Phase 1b/1c, and the Experiments screens on
fixtures. Both proceed.

### R13 — Signing key is unsealed on disk (OPEN, deliberate — decided 26 Oct 2026)
The ed25519 signing key is stored as raw bytes at mode 0600 in
`~/Library/Application Support/OpenLabbook/<LOGOS_PROFILE>/identity.key`.
Anyone who can read that file can sign commits as this node.

**Decided: accept this for now**, because sealing it means a passphrase prompt
on every single launch — `lgs basecamp launch` scrubs the profile tree, so
there is nowhere to cache an unlocked session — and that lands directly in the
two-profile test loop. docs/11-identity-design.md specifies the Argon2id +
ChaCha20-Poly1305 vault that replaces it.

**This is a blocker with a named trigger, not a vague intention. Seal the key
BEFORE any of these happen:**
1. a build is given to anyone outside this machine, or
2. any real (non-fixture) research data is entered, or
3. the app is pointed at anything other than a local testnet.

Also still true and worth not forgetting: sealing the key alone does **not**
protect content. Records sit in `store.json` in cleartext, so a file-copy
attacker reads the science regardless. "Private project" is currently a claim
about the network, not about the disk. Encrypting the store is a separate,
larger change that was considered and deferred at the same time.

### R21 — A synchronous core call from the UI thread can freeze it forever (found 28 Oct 2026)

Observed on alice: `olb_ui`'s main thread blocked for **over six minutes** in

```
QRemoteObjectSourceIo::onServerRead          <- handling a SLOT from QML
  OlbUiBackend::setCurrentProject
    OlbCore::set_current_project(QString const&, logos::CallError*, Timeout)
      qt_safe_poll                            <- parked here
```

while `olb_core` sat **idle** in `poll` at 0% CPU. So the request or its reply
was lost in transport, not slow to compute.

Two things matter about this.

**The timeout does not fire.** `logos_mode.h` defines
`explicit Timeout(int milliseconds = 20000)`, so the default is 20 s and the
generated consumer passes it. The call was still parked after six minutes. A
synchronous call issued from *inside* a QtRO event handler therefore does not
honour its own deadline — it is re-entrant on the event loop it needs in order
to receive the reply. Upstream bug; nothing in this repo can set a shorter
deadline and expect it to be respected.

**The blast radius is the whole UI**, because the blocked thread is the one
that repaints. The visible symptom is wherever the UI last drew: on 28 Oct the
"Create project" button read "Creating…" indefinitely, while the project itself
had been written to disk minutes earlier (commit `51586aebdcb8`). The frozen
control and the actual fault were in different code paths, which is worth
remembering before believing any "X is stuck" report.

**Not caused by the RLN log flood.** `liblogos_lez_rln_module` was writing
`storage.json` ~35–68×/second and accounted for 96% of the log, which made it
the obvious suspect. Killing it stopped the flood (line count frozen at
24,901) and the UI stayed blocked in the same frame. Ruled out by experiment.

**Mitigation available to us:** cut the number of synchronous round-trips per
user action. `refreshBoard()` alone makes eight (`board`, `fixtures_enabled`,
`notes`, `projects`, `profile`, `history`, `members`, `invites`), so every
refresh is eight chances to wedge. One core method returning a combined
document would reduce that exposure eightfold. Separately, a mutating action
must publish its result **before** refreshing anything, so a hung refresh
cannot also cost the person the outcome of work that already succeeded — that
ordering was briefly the other way round and is corrected.

### R20 — Scaffold does not resolve a dependency's own dependencies (found 27 Oct 2026)

`liblogos_rln_module` was captured in `scaffold.toml`, built, and installed into
both profiles — and never loaded. Its own `manifest.json` declares

```json
"name": "liblogos_rln_module",
"dependencies": ["liblogos_lez_rln_module"]
```

a **required** dependency that was not installed, so basecamp could not resolve
the module and skipped it. Nothing in the install output said so.

The cause is the scope of scaffold's dependency resolution: it walks the
`dependencies` / `optional_dependencies` of **`role = "project"`** sources only.
`liblogos_rln_module` is `role = "dependency"`, so its own requirements are
never read and the "unresolved dependency" fast-fail never fires. The gap
compounds the optional-dependency note recorded above `[modules.liblogos_rln_module]`:
delivery lists RLN as *optional*, so scaffold skipped it silently; RLN lists
lez-rln as *required*, so scaffold never looked.

**Fix:** capture the transitive dependency explicitly, at the same rev as its
sibling so the two are the versions that were built together:

```toml
[modules.liblogos_lez_rln_module]
flake = "git+https://github.com/logos-co/logos-rln-modules?dir=logos-lez-rln-module&rev=a814e7feda974a16eaa5dd50667b8fd843177f0e#lgx"
role = "dependency"
```

**Rule for this repo:** after installing any `role = "dependency"` module, read
the installed `manifest.json`'s `dependencies` array and confirm every name
appears in `[modules]`. A module that is installed is not necessarily a module
that loads, and basecamp does not report the difference. `lgs basecamp doctor`
does not catch this either — it checks drift between `[modules]` and on-disk
profile state, not whether each installed manifest's requirements are met.

### R19 — This machine has no login keychain (found 27 Oct 2026, BLOCKS the chosen store design)

The store-at-rest decision taken on 26 Oct was "macOS Keychain". It cannot
work here as configured:

```
$ security list-keychains      ->  only /Library/Keychains/System.keychain
$ security default-keychain    ->  A default keychain could not be found
$ ls ~/Library/Keychains/login.keychain-db  ->  No such file or directory
```

There is no login keychain, so there is nowhere for a generic password to
go. The app surfaces this as a system dialog: *"A keychain cannot be found
to store 'alice'"*, offering Cancel or **Reset To Defaults** — and Reset
rewrites the keychain search list for the whole user account, which is far
out of proportion to installing one app. Cancel.

This also explains two earlier symptoms that looked like our bugs: a read
from a second binary failing, and a write appearing to succeed into no
durable location.

Three ways forward, and this is the user's call because it changes the
posture they chose:

1. **Create a login keychain.** Restores the design as decided. It is a
   change to their account's security configuration, and a missing login
   keychain is unusual enough that the reason for its absence matters
   before recreating it.
2. **Env-var keys for dev** (`OLB_STORE_KEY_HEX_<PROFILE>`, implemented).
   Unblocks immediately and changes no system state. Encryption at rest is
   then only as strong as a file on the same disk as the data — which is
   the exact posture R13 warns about, acceptable only until one of its
   triggers fires.
3. **Passphrase mode**, which docs/11 §1.4 specifies as the default anyway
   and which needs no keychain. The most robust answer and the one the
   original design preferred; costs a prompt on every launch and an
   Argon2id implementation that is currently unwritten.

Nothing is chosen yet. Option 2 is implemented so work is not blocked while
the decision is open.

### R18 — A module-level wallet route EXISTS (found 27 Oct 2026; supersedes part of R12)

docs/07 task 6 and R12 both record that "Basecamp 0.3.0 ships no wallet
module, so there is no module-level route to the LEZ wallet." **That is no
longer true.** The Basecamp catalogue publishes `lez_core` 0.5.1, a
`type: core` module that ships `libwallet_ffi.dylib` and a real LIDL:

```
method send_generic_public_transaction(account_ids: [tstr],
        signing_requirements: [bool], instruction: bstr,
        program_id_hex: tstr, payer_account_id_hex: tstr,
        shard_program_account_ids_hex: [tstr]) -> tstr
method poll_transaction_status(tx_hash_hex: tstr) -> bool
method send_program_deployment_transaction(...) -> tstr
method create_account_public() / list_accounts() / get_balance(...)
```

`send_generic_public_transaction` takes a `program_id_hex` and an
`instruction` blob, which is exactly the shape `olb_registry.anchor` needs.
So `olb_core` could anchor directly instead of going through the host
runner, and `poll_transaction_status` gives the inclusion receipt R12
complained was missing — a tx_hash could finally be checked rather than
assumed.

**What blocks it is packaging, not architecture.** `lez_core` 0.5.1 ships
variants `darwin-arm64`, `linux-amd64`, `linux-arm64` and NO
`darwin-arm64-dev`. Scaffold's basecamp (`[repos.basecamp].attr = "app"`)
is a dev build and loads `<host>-dev` variants only, so installing it into
an `lgs basecamp launch` profile fails with:

```
Install failed: lez_core: Package does not contain variant for platform:
darwin-arm64-dev (package provides: darwin-arm64, linux-amd64, linux-arm64)
```

It installs cleanly into the separate release app at
`/Applications/LogosBasecamp.app`, whose data root is
`~/Library/Application Support/Logos/LogosBasecamp/` — that is where a
working copy already sits on this machine.

Three ways out, none free:

1. **Ask Logos to publish a `-dev` variant.** The right fix, and the only
   one that keeps the two-profile dev loop intact. Nothing to build.
2. **Move the whole stack to portable.** Our flakes already expose
   `lgx-portable`, so OLB could be hand-loaded into the release app
   alongside the wallet. Costs `lgs basecamp launch`, and with it the
   scripted alice/bob isolation every acceptance test is written against.
3. **Stay as we are.** Anchor from the host runner, which works today
   (R12). Costs nothing and blocks nothing in Phase 1b or 1c.

Taken: 3 for now, with 1 as the ask. Anchoring is not on the 1b/1c critical
path, and trading away two-profile testing for an in-app wallet we do not
yet call would be a bad exchange.

### R12 — LEZ anchoring end to end (RESOLVED 25 Oct 2026; re-verified per-project 26 Oct 2026; its wallet-route claim superseded by R18)
`lgs create --template lez-framework` ships `src/bin/run_lez_counter.rs` with
`// TODO: submit transaction via wallet` in every subcommand: it parses
arguments, loads the program ELF, prints a line and returns. Nothing reaches
the sequencer. `olb_registry`'s runner inherits that gap and says so on stderr
rather than printing a success line it has not earned.

**Resolved.** Anchors are on the local sequencer. Re-verified 26 Oct 2026
after the per-project PDA change (docs/05 §4), with **two** projects anchored
under one `program_id` to prove they no longer share a slot:

```
program_id  2439aae61197b93d1b5dd3ff3983136e3a5357c8a1679ceb9c6829e9df803f93

proj-alpha  tx c1fdb77213bc99e9201670c77f4809e9f29fdbff60255c8ed331e8d5cb989cf8
            Public/Akqd59AmFn1A3CjFd2TKyNmWtisq5ERuoNkUTAtiKm2M
            data 0700000000000000 aaaa…aa11   balance 0

proj-beta   tx 078d23681e6b852ffc87de46d9c4a59bf0bfd76aca5ad778867ff1b79eedf239
            Public/2WXZoi82H2h9Tde6v9uY5YpRBefmw9KtnjQDq7GzjshB
            data 0300000000000000 bbbb…bb22   balance 0
```

Two tags, two accounts, each holding its own `(count, root)`. Under the old
`literal("olb_registry")` seed both would have resolved to the same account and
the second anchor would have overwritten the first. The pre-change anchor
(`tx 3b1f9256…`, account `GU8Kt6X1…`) belongs to a now-orphaned `program_id`.

Sequencer: "Validated transaction ..., including it in block". `balance: 0`
confirms writing state to `data` rather than `balance` avoids the
conservation check that rejects minting.

Writing the submission was small: `lgs build client` generates
`OlbRegistryClient::anchor()`, which already does nonces, signing keys,
`Message`/`WitnessSet` and `send_transaction`. The runner only had to call it.

**The environment, not the code, was the work.** In order:
1. `logos-blockchain-pol` needs the circuits release — `lgs setup` fetches it
   to `.scaffold/circuits`; export `LOGOS_BLOCKCHAIN_CIRCUITS` at it.
2. `rzup install cpp` — the risc0 C++ guest toolchain was missing, so `ring`'s
   build script had no compiler.
3. `rzup install cargo-risczero` — needed for the container builder. It hits
   the GitHub API anonymously and gets rate-limited; `GITHUB_TOKEN=$(gh auth
   token) rzup install ...` works.
4. `DOCKER_DEFAULT_PLATFORM=linux/amd64` — the pinned
   `risc0-guest-builder:r0.1.97.0` image has **no arm64 variant**, so on Apple
   Silicon the build fails with "no match for platform in manifest". Emulated
   it takes ~17 min.
5. **`RISC0_SERVER_PATH` must point at `r0vm`.** Scaffold looks for it in the
   rzup extensions dir for the LEZ-pinned risc0 version, does not find the
   installed `~/.cargo/bin/r0vm`, and only warns. The sequencer then starts,
   serves RPC, and **dies on its first block** — "Collecting transactions from
   mempool, block creation" then "Main loop exited unexpectedly". Set it
   explicitly or localnet silently lasts about 15 seconds.

**Trap worth remembering: the embedded ELF is not the deployed ELF.**
`risc0_build::embed_methods()` embeds the *local* guest build
(`target/riscv-guest/...`) while `lgs deploy` deploys the *docker* build
(`target/riscv-guest-docker/...`). Their image ids differ, so a runner using
the embedded constant addresses a program the sequencer has never seen and the
transaction fails the execution check with `InvalidInput("Unknown program")` —
after being accepted and returning a tx_hash, so it looks like success. Pass
`--program-path <docker elf>` to the runner, or deploy the local build.

**Two anchors from one authority in the same block window: the second is
dropped.** Submitting `proj-alpha` and `proj-beta` back to back gave
`InvalidInput("Nonce mismatch")` on the second — both runs read the authority's
nonce before either transaction landed, so the second carried a stale one. The
sequencer logs `skipping it`; the runner had already printed a success line.
Resubmitting the identical anchor worked. **Consequence for the anchoring
queue:** submissions sharing a signing authority must be serialised, one
confirmed before the next is built, or anchors silently disappear. One flush
per authority at a time, and treat a `tx_hash` as a receipt only after reading
the account back.

**`cargo build` on its own cannot build the host runner on this machine.** The
`olb_methods` build script runs `risc0_build::embed_methods()`, which builds the
guest with the *local* risc0 toolchain, and `ring`'s C build then gets host
macOS flags (`-arch arm64`, `-mmacosx-version-min=…`, `-gfull`) handed to
`riscv32-unknown-elf-gcc`, which rejects all three. Use `lgs build`, which
routes the guest through the container builder and links the host crates
afterwards. Not worth fixing while `lgs build` works, but it does mean a bare
`cargo build`/`cargo test` at the workspace root fails for reasons that have
nothing to do with the code being changed.

Still open: `lgs localnet start` returns once the sequencer is up, and the
sequencer dies with the shell that spawned it; run it detached. There is no
deploy inclusion receipt — scaffold notes the program id is computed locally
from the submitted ELF, so deploy success is only "the wallet exited 0".

### R11 — The published Storage API reference does not match the shipped module (25 Oct 2026)
`logos-storage-module/latest/api_reference.html` documents **3.0.2**. The
version basecamp 0.3.0 ships — and that a running 0.3.0 instance on this
machine has installed — is **2.1.3**, and their APIs differ:

| | 3.0.2 (docs) | 2.1.3 (shipped) |
| --- | --- | --- |
| `uploadUrl` | `(filePath, chunkSize, advertise)` | `(filePath, chunkSize)` |
| `downloadToUrl` | `(cid, filePath, local, chunkSize, isPrivate, advertise)` | `(cid, filePath, local, chunkSize)` |
| `fetch` | `(cid, isPrivate, advertise)` | `(cid)` |
| completion events | typed fields | one `payload: tstr` holding JSON |
| also in 3.0.2 only | `getAdvertise`/`setAdvertise`, `collectMetrics`, `network()`, `loadConfigOrDefault` | — |

Coding against the website would have failed to compile or, worse, compiled
against the wrong arity. **Read the module's own `.lidl` instead** — every
module built by a current module-builder publishes `packages.<system>.lidl`,
and `nix build <flake>#lidl` gives the authoritative contract with full
parameter docs. The same applies to Delivery.

A related trap: scaffold's default `delivery_module` pin
(`BASECAMP_DEPENDENCIES`, rev `1fde1566`) publishes **no** `lidl` output at
all, so module-builder 0.3.0 refuses it — "lists dependencies that publish no
LIDL". Delivery v0.3.3 (`1d995bda`) does. Pin that.

Storage `init` also defaults `data-dir` to a **relative** `.cache/storage`,
which collides between the alice and bob profiles; set it absolute under
`instance_persistence_path`.

### R10 — The Rust path cannot carry a committed `.lidl` with record types (25 Oct 2026)
`docs/06` Conventions and `CLAUDE.md`'s decided-architecture table both say
`olb_core` "declares its contract in a **committed `olb_core.lidl`** with record
types ... not as JSON strings". Against module-builder 0.3.0 that is not
achievable on the Rust path, and the two decided items conflict with each other.

Evidence, from the pinned source:
- The `#rust` template is **Rust-first**: "the trait below IS the contract. The
  builder derives the `.lidl` from it (`codegen.rust.trait` in metadata.json) —
  no `build.rs`, no committed `.lidl`." Confirmed by building `olb_core`: the
  builder *emitted* `share/logos/olb_core.lidl` as an install sidecar, derived
  from `trait OlbCoreModule`. Nothing in the repo declares it.
- The trait can only express the scalar subset the template documents:
  `String`, `i64`, `u64`, `f64`, `bool`, `Vec<u8>`, `Option<T>`. There is no
  struct/record spelling, so an OLB record crosses the seam as a JSON string —
  exactly what `docs/06` wanted to avoid.
- **LIDL itself is not the limitation.** `lidl/ast.hpp` has `TypeDecl`,
  `FieldDecl` and `TypeExpr::{Primitive, Array, Map, Optional, Named}`, so
  records, arrays, maps and optionals are all in the language. A hand-written
  `.lidl` (the C++ path, as `chat_module` does) can use them.

So the choice is between two decided items, not between a doc and the source:
1. Keep `olb_core` in Rust (CLAUDE.md engineering rules) and accept JSON strings
   for record payloads — `docs/06`'s legacy envelope becomes the norm, not the
   exception.
2. Author `olb_core` in C++ with a committed `.lidl` to get typed records, and
   give up "Rust for `olb_core`".
3. Keep Rust, and revisit once a later module-builder lets a Rust module consume
   a hand-written `.lidl`. Not available at the 0.3.0 pin.

Phase 0 task 3 shipped under option 1 (`session_status() -> tstr`). **Unresolved
— needs a decision before Phase 1b defines the record format.**

What is confirmed working on the Rust path (read off the builder's doc-tests,
not guessed):
- dep wrappers in a `ui_qml` universal backend are Qt-typed, so `tstr` arrives
  as `QString`
- typed calls are synchronous and preserve snake_case
  (`modules().rust_ledger_module.add_entry`)
- a Rust events-trait `fn tally_changed` surfaces to subscribers as
  `onTally_changed`, hence our `session_changed` -> `onSession_changed`

### R9 — Nix module builds failed on macOS 14.5 (RESOLVED by OS update, 25 Oct 2026)
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

**Resolved.** The hypothesis was correct. On macOS 27.0.1 (build 26A434,
Darwin 27.0.0) a clean stock `#rust` probe walked straight through the failing
hook — `wrapping Qt applications ... wrapping .../logos-view-generator`, then
`postPatchMkspecs` — and `nix build .#lgx` exited 0 with a real `.lgx`. Zero
occurrences of SIGSEGV/exit 139 in the log. `olb_core` and `olb_ui` then both
built to `.lgx` on the same toolchain. No code change was needed; do not carry
a workaround for this.

Still true and unrelated: the linker warns `dylib (... qtbase-6.9.2 ... QtCore)
was built for newer macOS version (12.0) than being linked (11.3)` on every Qt
link. Cosmetic at this pin.

Separate issue found while re-testing, not a Logos problem: `cache.nixos.org`
resolves to four Fastly POPs and two of them crawl from this ISP (~150 KB/s,
stalling to <1 byte/sec and exhausting nix's 5 download attempts at 300 s each,
which fails the whole build at `rustc-1.89.0`). Measured: 151.101.129.91 and
151.101.1.91 slow; 151.101.65.91 and 151.101.193.91 fast — against ~1.6 MB/s to
Cloudflare, so it is peering, not bandwidth. Pinning the fast POP in
`/etc/hosts` fixed it. Note that `stalled-download-timeout` /
`download-attempts` in `~/.config/nix/nix.conf` are **ignored** — substitution
runs in the daemon and `trusted-users = root`, so those belong in
`/etc/nix/nix.conf`.

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
