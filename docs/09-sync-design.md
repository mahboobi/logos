# 09 — Peer-to-peer sync design

Answers Phase 0 tasks 8 and 9 (`docs/07`), and specifies the sync layer
`olb_core` builds in Phase 1b. Everything numeric here was read out of the
pinned source, not the published docs — `docs/08` R11 is the standing warning
about trusting the website.

Pins this document was written against:

| Thing | Pin | How to get the source |
| --- | --- | --- |
| `delivery_module` | 0.3.3, rev `1d995bda` | `nix build 'github:logos-co/logos-delivery-module/1d995bda…#lidl'` |
| `logos-delivery` (the Nim library) | v0.39.1, rev `37b4b800` | `nix flake prefetch 'git+https://github.com/logos-messaging/logos-delivery?rev=37b4b800acf18c7069c0de7beb61a8b91f274d10'` |
| `nim-segmentation` | rev `0593ef7c` | `nix flake prefetch 'git+https://github.com/logos-messaging/nim-segmentation?rev=0593ef7c…'` |
| `nim-sds` | rev `4b08d508` | `nix flake prefetch 'git+https://github.com/logos-messaging/nim-sds?rev=4b08d508…'` |
| Network | `logos.test`, cluster 2 | `{"mode":"Core","preset":"logos.test"}` — what `olb_core` already sends |

File paths below are relative to the `logos-delivery` source root unless named
otherwise.

---

## 1. The payload ceiling (Phase 0 task 8) — answered

**153,600 bytes (150 KiB), applied to the encoded `WakuMessage` protobuf, not
to the payload alone.**

Evidence:

- `logos_delivery/waku/factory/networks_config.nim:114` — `LogosTestConf` sets
  `maxMessageSize: "150KiB"`, `clusterId: 2`, `rlnRelay: false`, `mix: true`,
  `shardingConf: AutoSharding(numShardsInCluster: 8)`. (`status.prod` sets
  `1024KiB`; `twn` and `logos.dev` take the library default, which is also
  150 KiB — `logos_delivery/waku/waku_core/message/default_values.nim:5`,
  `DefaultMaxWakuMessageSizeStr* = "150KiB"`.)
- `logos_delivery/waku/waku_relay/protocol.nim:578` —
  `let messageSizeBytes = msg.encode().buffer.len`, then
  `if messageSizeBytes > w.maxMessageSize: return err(...)`. So the budget is
  consumed by `payload` **plus** `contentTopic`, `timestamp`, `meta`,
  `ephemeral` and (where RLN is on) `rateLimitProof`. On `logos.test` RLN is
  off, so overhead is small: topic string + ~30 bytes of protobuf framing.
- `maxMessageSize` is caller-overridable per node
  (`messagingOverrides`/`kernelConf`), but raising it locally does not help —
  every relaying peer on the shard validates against *its own* preset and drops
  the message. Treat 150 KiB as fixed.

**Usable budget for `send`: 147,456 bytes (144 KiB).** That is 150 KiB minus a
2 KiB allowance for the envelope. OLB's own design budget is far below this
(§5).

**The failure mode is a lie, and this matters more than the number.** An
oversized publish is dropped inside GossipSub before it reaches a peer;
`WakuRelay.publish` then sees `relayedPeerCount <= 0` and returns
`err(NoPeersToPublish)` (`waku_relay/protocol.nim:697`). The module surfaces
that as a `messageError` whose text says there were no peers. **An oversize
commit is indistinguishable from being offline.** `olb_core` must therefore
check the frame size itself before calling `send`, and refuse with its own
error, rather than relying on Delivery to report it.

**Base64 does not eat the budget.** `send` base64-encodes the payload only to
cross the FFI boundary (lidl `send` doc comment); the wire payload is the
decoded bytes. It does mean a 144 KiB send pushes ~192 KiB of JSON across the
`olb_core → delivery_module` IPC seam. Whether logos-core caps that is the one
thing on this subject still unmeasured — see §10 E1.

**`channelSend` is not bounded at 150 KiB.** Reliable channels segment first
(`logos_delivery/channels/reliable_channel.nim:198-250`: segmentation → SDS →
encryption → dispatch), so the per-Waku-message ceiling applies to a *segment*:

| Constant | Value | Source |
| --- | --- | --- |
| `DefaultSegmentSizeBytes` | 102,400 ("fits the 150 KiB cap of Logos Delivery with room to spare") | nim-segmentation `segmentation/segmentation_config.nim:9` |
| `SegmentHeaderMaxBytes` | 128 | same file, :34 |
| `ShardAlignment` | 64 | nim-segmentation `segmentation/parity.nim:16` |
| chunk per segment | **102,272** = `(102400-128) div 64 * 64` | `segmentation_handler.nim:115` |
| `DefaultMaxTotalSegments` | 256 (data + parity) | `segmentation_config.nim:17` |
| **max one `channelSend`** | **26,181,632 bytes (24.97 MiB)** | product of the two above; `segmentation_handler.nim:173` rejects more |
| `DefaultParityRate` | 0.0 (parity off) | `segmentation_config.nim:12` |
| `DefaultMaxBufferedBytes` | 32 MiB across *all* incomplete inbound sets | `segmentation_config.nim:24` |
| `DefaultReconstructionTimeoutSeconds` | 300 | `segmentation_config.nim:14` |
| SDS's own `MaxMessageSize` | 1 MiB | nim-sds `sds/types/reliability_config.nim` |

So `channelSend` tolerates ~25 MB, but a receiver has 32 MiB of reassembly
buffer for *everything* in flight and drops a set that goes 300 s without a new
segment. Pushing megabytes through it is possible and is still wrong: that is
what Storage is for (CLAUDE.md constraint 2).

**No rate limit on `logos.test`.** `LogosTestConf` has `rlnRelay: false`, and
the messaging rate-limit manager defaults to `enabled: false`
(`logos_delivery/messaging/rate_limit_manager/rate_limit_config.nim:29`). The
`messageQueued` event documented in the lidl will not fire on this network.
`docs/08` R5's note that publishing "**is** rate-limited by epoch quota" is
true for RLN networks (`twn`), not for ours. Do not design around a quota we do
not have — but do not depend on its absence either, since `logos.dev` and `twn`
differ.

**Sharding, which the topic scheme has to respect.** Auto-sharding derives the
shard from the content topic's *application and version segments only* —
`sharding.nim:20`: `let bytes = toBytes(topic.application) &
toBytes(topic.version)`. Every `/olb/1/…` topic therefore lands on the same one
of cluster 2's 8 shards, no matter what the `name` segment says. Two
consequences:

1. Splitting traffic across 16 bucket topics does **not** spread relay load. It
   only changes which topics a node subscribes to, i.e. how much traffic it has
   to receive and discard.
2. Phase 0's `/open-labbook/1/records/json` has a different `application`
   segment, so it is on a *different shard* from `/olb/1/…`. Pick one
   application string now and keep it (§4).

---

## 2. SDS reliable channels vs the bucket-topic design (Phase 0 task 9) — answered

### What SDS actually provides

From nim-sds `sds/sds.nim` and `sds/types/*`: Lamport timestamps, a causal
history of the last `DefaultMaxCausalHistory = 10` message ids per message, a
rolling bloom filter (`DefaultBloomFilterCapacity = 10000`, error rate 0.001)
carried in every message as the sender's "I have seen these" summary,
acknowledgement by presence in a peer's causal history or bloom
(`isAcknowledged`), retransmission, periodic sync messages every 30 s, and
SDS-R repair requests (`repairTMin` 30 s, `repairTMax` 300 s,
`maxRepairRequests` 3). Plus the segmentation layer in §1.

### The three findings that decide this

**(a) SDS reliability has a ~5 minute horizon, and then it lies.**
`sds/sds.nim:437-483`: every `bufferSweepInterval` (60 s) an unacknowledged
message past `resendInterval` (60 s) is retried, up to `maxResendAttempts`
(5). On the sixth pass the entry is dropped from the outgoing buffer **and
`onMessageSent` is fired for it anyway**. A caller that trusts
`channelMessageSent` as proof of delivery will be wrong about any message whose
recipient was away for more than about six minutes. SDS is a jitter-and-packet-
loss repair layer, not a store-and-forward queue.

**(b) The `channelId` and `senderId` are plaintext on the wire.** The delivery
module's `channelCreate(channelId, contentTopic, senderId)` has no key
parameter, so it calls `createReliableChannel` with
`encryption = Opt.none(ChannelCrypto)`
(`logos_delivery/channels/api/channel_lifecycle.nim:41-50`) and the SDS
protobuf — which carries `channelId`, `senderId`, `lamportTimestamp`,
`causalHistory` message ids and the bloom filter — goes out unencrypted.
Anyone on the shard sees it. A naive `channelId = project_id` reintroduces
exactly the leak the bucket topics in `docs/01` §5 exist to prevent, and a
`senderId = ed25519 fingerprint` publishes a stable cross-project identity.
Both are fixable by derivation (§4) and must be fixed.

**(c) Channel state does not survive a Basecamp relaunch, whatever the lidl
says.** The lidl promises "persisted channel state survives `channelClose`",
and the library does wire SDS to the node's persistency job
(`channel_lifecycle.nim:19-30`). But that store lives under the delivery
module's instance path, inside the tree `lgs basecamp launch` scrubs on every
launch (`docs/08` R6, CLAUDE.md constraint 8). **Assume every SDS buffer, bloom
filter and half-reassembled segment set is empty at every launch.** Channel
persistence buys us nothing across restarts; it buys us something across a
`channelClose`/`channelCreate` within one session.

### Comparison

| | Raw bucket topics (`docs/01` §5) | SDS reliable channels |
| --- | --- | --- |
| Ordering | None. We order by the commit DAG. | Lamport + causal history, but over SDS messages, not our commits. We still order by the DAG. |
| Delivery guarantee | None. Gossip best effort. | Retransmit + ack + repair, for ~5 min, then a false "sent" (finding a). |
| Large payloads | We write a chunker, or stay under 144 KiB. | Free, up to ~25 MB (§1). **The strongest single argument.** |
| Offline for a week | Lost. Store retention defaults to `time:172800` = **2 days** (`waku/factory/conf_builder/store_service_conf_builder.nim:13`), and `storeQuery` needs an explicit store-peer multiaddr and is flagged unstable (`docs/08` R5). | Also lost. The channel's buffers are gone (finding c) and the retries expired (finding a). |
| New member catching up | Snapshot from Storage + delta. | Snapshot from Storage + delta. Identical. |
| Privacy of who-talks-to-whom | Good: a topic name says only "some OLB node, bucket k". | Worse by default, equal after deriving the ids from the project key (finding b, §4). |
| Member removal (CLAUDE.md 6) | Nothing to remove; removal = key rotation. | Nothing to remove either: a channel has no membership list, no join, no leave, no admin. Removal = key rotation, and the new epoch's `channelId` is not computable without the new key — so a removed member cannot even find the new channel. **Channels are compatible with removal in the way Chat groups are not.** |
| Cost per project | 1 subscription shared by ~N/16 projects. | 1 channel per project: an SDS buffer, a bloom filter, a segment-cleanup loop and a sync message every 30 s. 20 projects = 20 sync messages per 30 s per node. |

### Recommendation

**Use SDS reliable channels, one channel per project per key epoch, carried on
the shared bucket content topics — and keep the bucket-topic scheme from
`docs/01` §5 underneath them.** They are not alternatives; the channel is the
reliability layer and the bucket topic is the addressing-and-privacy layer. The
`channelId` and `senderId` must be derived from the project key so the
plaintext SDS header is opaque to non-members.

Reasoning, in order of weight:

1. Segmentation removes the single largest piece of protocol code we would
   otherwise write, and removes the class of bug where a commit that grew past
   144 KiB fails as "no peers" (§1).
2. In-session retransmission and SDS-R repair cover the realistic failure —
   a laptop lid closed for ten minutes, a flaky café network — which is
   precisely where raw gossip drops commits silently and where our own 60 s
   head-exchange would otherwise be the only repair.
3. It costs nothing in privacy once the ids are derived, and it is strictly
   better than Chat groups on membership.
4. Everything channels do *not* cover — a week offline, a new member, a
   relaunch — is covered by snapshot-to-Storage, which CLAUDE.md constraint 4
   requires us to build regardless. So choosing channels does not let us skip
   work we would otherwise have to do; it only removes work.

The head-exchange loop from `docs/05` §5 stays. It is what makes finding (a)
survivable: a 60 s heads frame plus `want` is our store-and-forward, and SDS is
the fast path underneath it.

### What would change my mind

- **Demux fails.** If two channels on one content topic do not reliably route
  by `channelId`, channels force one content topic per project and the bucket
  privacy is gone. The source says this works (`topicStillUsed` in
  `channel_lifecycle.nim:31` exists precisely because channels share topics),
  but it is unproven on our pin. Experiment E2.
- **Segment overhead dominates small projects.** If the 30 s sync message per
  channel, times the number of projects a member is in, measurably crowds a
  two-profile localnet, drop to raw `send` for projects with ≤ 3 members and
  keep channels for large ones. Experiment E3.
- **`channelSend` turns out to cap below ~128 KB** on our pin, which would mean
  segmentation is not actually mounted. Then channels buy only (2), and the
  trade becomes marginal. Experiment E2.
- **A later module-builder exposes channel encryption.** Then the derivation
  trick in §4 is unnecessary and channels win outright.

---

## 3. What has to change in `olb_core` first

The Phase 0 code is a single-node append log, not a DAG. Three concrete
corrections before any of this lands:

1. **`commit()` takes one global linear parent** —
   `olb_core/rust-lib/src/lib.rs:226`, `parent` = the last entry in
   `store["commits"]`, shared by every project. It must become per project and
   per branch, and `parent` must become `parents: []` as `docs/05` §2 specifies.
2. **`SEEN_HASHES` is an unbounded `Vec<String>` scanned with `contains()`** —
   `lib.rs:151` and `:1045`. O(n) per inbound message and it grows forever.
   Replace with a bounded ring + `HashSet` (§7).
3. **The record topic is `/open-labbook/1/records/json`** — `lib.rs:19`. A
   different `application` segment means a different shard (§1). Rename to the
   `/olb/1/…` family in the same change, and keep the old topic only as a
   dev-only channel until Phase 1b cuts over.

Items 1 and 2 overlap with `docs/10-vcs-design.md`, which specifies the commit
DAG itself. Where the two disagree, `docs/10` wins on the DAG and this document
wins on the wire; neither should restate the other.

---

## 4. Naming

### Content topics

Waku format is `/<application>/<version>/<name>/<encoding>`
(`waku_core/topics/content_topic.nim`). Fixed choice: `application = olb`,
`version = 1`.

| Topic | Carries | Bucket |
| --- | --- | --- |
| `/olb/1/sync-<b>/proto` | project commits, heads, wants, snapshot announcements | `b` = low nibble of `SHA-256(project_id)`, hex `0`–`f` |
| `/olb/1/invite-<b>/proto` | sealed invites (`docs/04`) | `b` = low nibble of `SHA-256(invitee ML-KEM-768 public key)` |
| `/olb/1/replica/proto` | replica announcements, pin acknowledgements | none |

`encoding` stays `proto` — the parser only needs four segments, and our frames
are ciphertext, not JSON, on the wire. A node subscribes to the `sync-<b>` and
`invite-<b>` topics it needs and no others; a replica subscribes to all 16
sync buckets plus `/olb/1/replica/proto`.

16 buckets is the `docs/01` §5 number and stays. Note what it does and does not
buy, now that §1 has established that sharding ignores the `name` segment: it
hides which labs collaborate (good) and reduces how much other projects' traffic
a node decrypts and discards (good); it does **not** spread network load.

### Channels

One channel per project per key epoch, on that project's bucket topic:

```
channel_key = HKDF-SHA256(project_key_epoch_k, info = "olb.channel.v1")
channelId   = base32-nopad( HMAC-SHA256(channel_key, project_id) )[0..26]
senderId    = base32-nopad( HMAC-SHA256(channel_key, member_signing_pk) )[0..26]
```

- Only a member can compute either. A non-member on the shard sees two opaque
  27-character strings.
- `senderId` is a **per-project pseudonym**: the same member in two projects is
  not linkable without one of the project keys. This is why it is not the
  ed25519 fingerprint.
- Both rotate with the key epoch. Key rotation on member removal therefore
  moves the project to a channel the removed member cannot name (§2, removal
  row). After rotation, `channelClose` the old channel and `channelCreate` the
  new one; write a fresh snapshot under epoch k+1 in the same step (§6).
- Chat addresses appear nowhere in any of this (CLAUDE.md constraint 5).

### Record streams

**Do not give a record stream its own topic or channel.** Stream is a field
inside the frame (`"s"`, §5), demuxed in `olb_core`. Rationale: the shard is
fixed either way, each extra channel costs a sync message every 30 s, and the
channel count would otherwise multiply by the number of projects a member is in.

---

## 5. Wire framing

### Outer frame — binary, not JSON

`docs/08` R10 constrains the **IPC seam** (records cross as JSON strings
because the Rust LIDL path has no struct type). The wire is internal to
`olb_core`, so it is binary and small:

```
byte  0      : frame version, 0x01
byte  1      : key epoch (u8, wraps at 256 — a project rotating 256 times
               is out of scope; assert and refuse)
bytes 2..25  : XChaCha20-Poly1305 nonce (24 bytes, random per frame)
bytes 26..   : ciphertext || tag, AEAD over the canonical-JSON plaintext,
               with bytes 0..1 as additional authenticated data
```

Key: the project key at that epoch for project-level streams; the idea key for
a frame whose records all belong to one idea (`docs/01` §5, idea-level keys).
Which key is in use is implied by the stream and resolved from the plaintext
`project`/`idea` fields after decryption — a frame we cannot decrypt is
silently discarded, which is also what happens to every other project's traffic
in the same bucket.

### Plaintext — canonical JSON (RFC 8785), as `docs/05` already requires

```json
{ "v": 1, "p": "olb:proj:…", "s": "commit", "b": { … } }
```

`s` is one of `commit`, `heads`, `want`, `snapshot`, `pin`.

### Commit frames

The commit object itself (`docs/05` §2 — ids, versions, message, signatures)
always travels inline. It is small and it is what the DAG needs. The record
*versions* a commit creates travel inline only if they fit:

```json
// inline, when canonical(commit) + canonical(records) <= 49152 bytes
{ "s": "commit",
  "b": { "kind": "inline", "commit": { …docs/05 §2… }, "records": [ … ] } }

// manifest, otherwise
{ "s": "commit",
  "b": { "kind": "manifest", "commit": { …docs/05 §2… },
         "records_cid": "logos:…", "records_sha256": "…", "records_bytes": 318422 } }
```

- **Inline ceiling: 49,152 bytes (48 KiB) of plaintext.** That leaves more than
  3× headroom under the 144 KiB `send` budget after AEAD expansion (+40 bytes),
  SDS wrapping (bloom filter plus up to 10 causal-history entries) and the
  segment header — and it is sized for the common case, not the limit, because
  every frame is relayed to every node on the shard whether or not it can read
  it.
- **Manifest path:** the record versions are packed as one canonical-JSON
  array, encrypted with the same key, uploaded with
  `storage_module.upload_url(path, 65536)`, and the frame carries the CID plus
  a SHA-256 of the *plaintext* so a receiver can verify after decrypting.
- **Attachments are never inline, ever.** They are already CIDs in the record
  (`docs/05` §1). CLAUDE.md constraint 2.
- **No bespoke chunker.** A frame either fits one `channelSend` or it becomes a
  manifest. Segmentation (§1) is a safety net for the 48–144 KiB band, not a
  transport we design around.
- `olb_core` measures the encoded frame before sending and refuses with its own
  typed error if it exceeds 144 KiB — because Delivery would report it as "no
  peers" (§1).

### Heads frame — every 60 s, and immediately after every local commit

```json
{ "s": "heads",
  "b": { "branches": { "main": "<commit id>", "alt-buffer": "<commit id>" },
         "tags": { "h1-preregistered": "<commit id>" },
         "snapshot": { "cid": "logos:…", "at_commit": "<id>", "seq": 1240 },
         "have_count": 317 } }
```

### Want frame — a peer answers by re-sending `commit` frames

```json
{ "s": "want", "b": { "commits": ["<id>", "…"], "max": 64 } }
```

Re-sent commits are byte-identical and dedup by id (§7), so answering a `want`
is always safe, including when three peers answer it at once.

### Pin frame — on `/olb/1/replica/proto`

`docs/08` R4 established there is no network replica count, so OLB counts
acknowledgements itself:

```json
{ "s": "pin", "b": { "cid": "logos:…", "have": true, "at": "RFC3339" } }
```

A member or replica emits one after `storage_module.fetch(cid)` succeeds and
`exists(cid)` confirms it. The UI's copy count (`files.status`, `docs/01` §4) is
the number of distinct `senderId`s that have acknowledged within the TTL
window. Because Storage caches carry a TTL (`block-ttl`, default 30 d — R4),
"pinned" means re-fetched on a schedule: a weekly sweep re-fetches every project
CID and re-emits its pin frame.

---

## 6. Catch-up and snapshots

### Snapshot format

Written to Storage, encrypted with the project key at the current epoch,
canonical JSON, and signed:

```json
{ "fmt": "olb-snapshot/1",
  "project": "olb:proj:…",
  "key_epoch": 3,
  "at": "RFC3339",
  "seq": 1240,
  "refs": { "branches": { "main": "<id>", "…": "…" },
            "tags": { "h1-preregistered": "<id>" } },
  "commit_index": [ { "id": "…", "parents": ["…"], "at": "RFC3339" } ],
  "records": [ { …current version of every record, docs/05 §1… } ],
  "anchors": { "205": { "root": "…", "proofs": { "<commit id>": ["…"] } } },
  "members": [ { "key": "<ed25519 pk>", "role": "lead", "since_epoch": 1 } ],
  "prev_snapshot_cid": "logos:… | null",
  "author": "<ed25519 pk>",
  "sig": "<ed25519 over the canonical bytes of everything above>" }
```

- `commit_index` carries the **whole DAG shape by id**, not commit bodies. That
  is what lets a new member verify that the records it was handed correspond to
  a history, and lets it `want` only the bodies it actually needs.
- `records` holds the *current* version of each record. History bodies are not
  in the snapshot; they are fetched by `want` on demand when a user opens
  `docs/03` S6 History. A node is fully functional without them.
- Idea-scoped records stay wrapped under their idea key *inside* the snapshot,
  so a reviewer-scoped snapshot is a later addition, not a redesign.
- `prev_snapshot_cid` chains snapshots, so a verifier can walk back and an
  auditor can check that no snapshot silently dropped a record.

### When a snapshot is written

Confirms `docs/08` D4 and extends it:

- every 50 commits, **or** 24 h since the last one, whichever comes first;
- **immediately** on project creation, on any membership change (a removal
  rotates the key, and the new snapshot must be re-encrypted under epoch k+1 or
  the removal is cosmetic), and at the `docs/05` §4 key moments —
  pre-registration, milestone release, publication;
- by the Lead's node by default; any member whose `seq` exceeds the announced
  snapshot by more than 100 commits writes one itself after a random 0–60 s
  delay, so a project whose Lead is away does not go un-snapshotted. Duplicate
  snapshots are harmless — they are content-addressed and chained.

Every member node and the replica `fetch()` the last 3 snapshot CIDs; the
replica keeps all of them. Pinning is the weekly re-fetch sweep from §5.

### Catch-up algorithm

Run at `olb_core` start and on every `connectionStateChanged` → `Connected`:

1. Load the durable store (`docs/08` R6 path) and compute local branch heads
   per project.
2. Subscribe the needed `sync-<b>` and `invite-<b>` topics. `channelCreate`
   each project's channel. Assume SDS state is empty (§2 finding c).
3. Publish a `heads` frame. Collect peers' `heads` for 10 s.
4. For every peer head not in the local `commit_index`, send `want` in batches
   of 64. Apply arrivals per §7.
5. Escalate to the snapshot if **either** no peer answered in 10 s **or** a
   peer's announced `snapshot.seq` exceeds ours by more than 200 commits:
   `storage_module.download_to_url(cid, …)`, verify `sig` and the
   `prev_snapshot_cid` chain back to one we already trust, load, then `want`
   the delta from the snapshot's head to the peers' heads.
6. A brand-new member always enters at step 5, using the snapshot CID carried
   in the invite (`docs/04`). **A join costs one Storage download plus a delta —
   never a message replay.** This is the direct answer to "cost of a new member
   catching up", and the reason it does not depend on the Delivery Store
   existing at all (CLAUDE.md constraint 4).
7. Store backfill is accepted and never relied on. `messageReceived` with
   `source: "history"` is processed exactly like `"live"` — it only shortens
   step 4. `storeQuery` is **not** called in Phase 1b: it needs an explicit
   store-peer multiaddr, it is marked "use at your own risk" in the lidl, and
   the default retention it would query is 2 days (§2).

A member offline for a week therefore recovers through step 5, always. There is
no path in which message history gets them there.

---

## 7. Dedup and ordering

**The commit DAG is the source of truth.** Three dedup layers, all needed:

1. **`messageHash`** — drops gossip duplicates and Store replays. The lidl is
   explicit that upstream suppresses duplicates "only for a few minutes and
   only in memory". Implementation: a `HashSet<[u8;16]>` of truncated hashes
   plus an insertion-ordered ring of 8192, evicting oldest. In-memory only;
   layer 2 makes losing it on restart harmless. This replaces the unbounded
   `Vec` + `contains()` at `lib.rs:151`.
2. **Commit id** — `id` is SHA-256 over the canonical, domain-separated
   preimage (`lib.rs:260`, `COMMIT_DOMAIN`), so the same commit always has the
   same id on every node. Applying a known id is a no-op. **This is the
   authoritative layer and it is idempotent across restarts.**
3. **Record version** — a commit whose `changes[].from_version` does not match
   the version we hold is not applied; it is parked as if a parent were missing.

**Ordering.** Messages arrive out of order and replayed; the DAG absorbs both.

- A commit whose `parents` are all known is validated and applied immediately.
- A commit with an unknown parent goes into a **pending set** keyed by each
  missing parent id, and a `want` is issued for those ids. When a parent
  applies, every pending child keyed on it is retried — the same shape SDS uses
  for `missingDeps`. Park timeout 10 min → re-`want`; after 3 rounds → snapshot
  catch-up (§6 step 5).
- **Never order by `timestamp`.** Wall clocks differ between members and the
  field is display metadata. For the linear log `docs/05` §6 renders,
  topologically sort the DAG and break ties by `(timestamp, id)` so every node
  draws the same lanes.
- Validation order before apply, per `docs/05` §5: signature → author is a
  member holding a role permitted to make that change *at that key epoch* →
  parents known → version continuity. Any failure: drop, log, never partially
  apply. A commit signed by a member removed before the commit's key epoch is
  invalid.

---

## 8. Conflict: two members commit on the same parent

This is normal, not an error, and nothing is ever discarded.

1. **Both commits are kept.** The branch now has two heads. `olb_core` picks a
   deterministic *display* head — greatest `(topological depth, id)` — so two
   nodes never disagree about what the UI shows, but it does not merge silently.
2. **Disjoint changes auto-merge.** If the two sides touch different records,
   or different fields of the same record, `olb_core` builds a merge commit with
   both heads as `parents`. `docs/05` §3's rule is "never auto-merge conflicting
   fields", which bounds auto-merge to exactly this case.
3. **Protection still applies.** If the project sets N > 0 approvals for
   `main`, an auto-mergeable divergence on `main` produces a *proposed* merge
   commit held locally and surfaced as a merge request; it is published only
   once the required approvers have co-signed (`co_signers`, `docs/05` §2). On
   an unprotected branch it is published immediately.
4. **Same field, same record → diverged.** The project enters a `diverged`
   state: an amber "History diverged — resolve" panel (amber = needs attention,
   CLAUDE.md UI rules), both lanes rendered in `vcs.log`, and resolution is a
   human merge commit through the field-by-field view (`docs/08` D9). Work
   continues on whichever head the user is on; new commits are legal on both.
5. **Resolution is always a new commit.** History is never rewritten, including
   here. A restore is a new commit that sets fields back (`docs/05` §3).

Snapshots during divergence record **both** heads in `refs.branches` as
`"main": ["<id>", "<id>"]` — the one place a ref holds a list. A node loading
such a snapshot comes up diverged, which is correct: the divergence is real and
hiding it in the snapshot would resolve it by accident.

---

## 9. Implementation order

Each step is independently testable on `alice` + `bob` (plus `carol` and
`replica` where named).

| # | Task | Done when |
| --- | --- | --- |
| 1 | Fix `commit()`: per-project, per-branch heads; `parents: []`; `changes[]` per `docs/05` §2. Bounded `SEEN_HASHES`. Topic rename to `/olb/1/…`. | Existing Phase 0 acceptance still passes; the store holds a real DAG for two projects. |
| 2 | Frame codec: encode/decode §5, AEAD, canonical JSON, the 144 KiB refusal, round-trip property tests. | A frame survives encode → decode byte-identically; a 200 KiB frame is refused locally with OLB's own error, never sent. |
| 3 | Bucket-topic subscribe + raw `send` for `commit`/`heads`/`want`. Pending set and validation pipeline (§7). | A commit made on alice applies on bob in < 10 s; bob restarted mid-stream converges via `want`, not via replay. |
| 4 | Channels: derived `channelId`/`senderId` (§4), `channelCreate` per project, `channelSend` for commit frames, `channelMessageReceived` wired into the same pipeline as `messageReceived`. Keep raw `send` for `heads`/`want`. | Two projects on one bucket topic demux correctly (E2); a 300 KB manifest-free frame crosses intact. |
| 5 | Snapshots: write, encrypt, upload, announce, verify, load (§6). Weekly re-fetch sweep and pin frames (§5). | carol, launched fresh with only an invite, reconstructs the full board from the snapshot CID with Delivery's Store unreachable. |
| 6 | Divergence and auto-merge (§8), including the held proposed merge under branch protection. | alice and bob commit to the same parent offline; on reconnect, disjoint edits auto-merge, same-field edits surface the amber panel on both. |
| 7 | Key rotation path: new epoch → new channel → fresh snapshot → old channel closed. | carol removed on alice can no longer decrypt, cannot compute the new `channelId`, and sees no further traffic. |

Phase gate, per CLAUDE.md: nothing above is done until its row passes on two
profiles.

---

## 10. Open risks and the experiments that close them

**E1 — the IPC seam's own payload cap. Unmeasured.** `send` moves the payload
base64-encoded as JSON across `olb_core → delivery_module`. A 144 KiB payload is
~192 KiB of JSON. Nothing in the module contract states a cap, and §1's number
is a *network* limit, not an IPC one. Experiment: from `olb_core`, `send` a
payload of 1, 16, 64, 128, 144, 160 and 512 KiB on a dev topic and log which
return `Ok`, which emit `messageError`, and which fail at the call boundary
before any event fires. The last category is the IPC cap. Until measured, treat
48 KiB (§5) as the design ceiling and do not rely on the 48–144 KiB band.

**E2 — channel demux and real segmentation.** Create two channels with
different derived ids on one `sync-<b>` topic; send on each; assert each
`channelMessageReceived` carries the right `channelId` and that a third,
unknown-channel message is not delivered. Then `channelSend` 300 KB and 2 MB
payloads and assert reassembly. Blocks step 4; a failure sends us back to raw
`send` plus our own chunker.

**E3 — per-channel overhead at scale.** 16 channels on one node, idle, for
10 minutes; count messages via `collectOpenMetricsText()`. If the 30 s sync
messages dominate the link on a two-profile localnet, apply the small-project
exception from §2.

**R1 — `messageQueued` is dead code on `logos.test`.** Rate limiting is off
(§1). Handle the event anyway, because `logos.dev` and `twn` differ and the
preset is a one-word change.

**R2 — `mix: true` in the preset.** `LogosTestConf` mounts mix nodes, and
`anonymityLevel` above `"None"` routes sends through it with a much smaller
packet budget (the library carries an "exceeds max msg size of 3966 bytes"
path). OLB leaves `anonymityLevel` at the default `"None"`, so this does not
apply — but anyone who turns on anonymity will find the frame sizes in §5
invalid. Flagged, not designed for.

**R3 — Snapshot write is a single point of staleness.** The "any member 100
commits ahead writes one" rule in §6 is a guess at the right threshold. Revisit
once a real project's commit rate is known.

**R4 — `key_epoch` as one byte.** Fine for the foreseeable; asserted, not
silently wrapped.

**R5 — the `senderId` pseudonym is stable within an epoch.** An observer on the
shard can count distinct senders per channel and watch activity patterns, even
without the key. Traffic analysis of that shape is out of scope for Phase 1b
and should be named in the Phase 4 security review.

**R6 — nothing here depends on the Delivery Store, by design.** That is the
intent of CLAUDE.md constraint 4, and it is worth restating as a test: unplug
Store entirely and every acceptance row in §9 must still pass.
