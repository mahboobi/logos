# 11 — Identity, membership and key management

Makes `docs/04-invite-flow.md` implementable. Scope: the durable-identity gap
left open by `docs/08` R6, plus all of Phase 1c (`docs/07` §Phase 1c).
Written 26 Oct 2026. Where this doc and `docs/04` disagree, this doc wins and
§7 says why.

Nothing here is built yet. `olb_core` currently persists a **raw, unsealed**
32-byte ed25519 seed at `identity.key` (see `load_or_create_key()`), which its
own comment calls a developer-build posture superseded by this document. §1.6 is
the migration.

---

## 1. Durable identity

### 1.1 The gap, precisely

`lgs basecamp launch <profile>` scrubs the profile tree on every launch, so
`instancePersistencePath()` cannot hold anything long-lived (`CLAUDE.md` 8,
`docs/08` R6). The durable store therefore lives outside it:

```
~/Library/Application Support/OpenLabbook/<LOGOS_PROFILE>/
  store.json      # projects, notes, profile, commit log  (exists)
  identity.key    # 32 raw bytes, mode 0600                (exists — to be replaced)
  identity.olbk   # sealed vault                            (this doc)
```

Before `load_or_create_key()` landed, every launch produced a new ed25519 key,
so the `author` on every commit changed between runs and the signed history was
incoherent. That is fixed; what is **not** fixed is that the key is written in
the clear. Anyone who can read one file is permanently that researcher.

### 1.2 One master seed, three keys

Seal one secret, derive the rest. `ms` = 32 bytes from `OsRng`.

| Key | Derivation | Purpose |
| --- | --- | --- |
| Ed25519 signing | seed = `ms` verbatim | signs records, commits, invites, approvals |
| ML-KEM-768 sealing | `(d‖z)` = `Expand(info="olb-seal-v1", 64)`, deterministic keygen (FIPS 203) | receives sealed project and idea keys |
| Inbox tag (16 B) | `Expand(info="olb-inbox-v1", 16)` | private inbox topic bucket (`docs/04` §3) |
| Store DEK (32 B) | `Expand(info="olb-store-dek-v1", 32)` | encrypts records at rest — Phase 1b/2, see §1.7 |

`prk = HKDF-Extract(salt = "olb-kdf-v1", ikm = ms)`; `Expand` is
HKDF-SHA256-Expand over that `prk`.

Two consequences worth stating. The signing seed is `ms` **verbatim**, not
`Expand(…)`, purely so the existing `identity.key` migrates without changing
anyone's fingerprint (§1.6); it costs nothing, because ed25519 hashes its seed
internally and HKDF-Extract is a separate domain. And because every key is a
function of `ms`, **backup and recovery are a 32-byte secret** — no separate
export flow per key, and losing `ms` loses the identity outright.

Recovery code = Crockford base32 of `ms ‖ CRC-32(ms)`, 58 characters in groups
of 4, shown once at creation and on demand from Settings. It is the identity, in
plaintext, on paper: the UI must say so in those words.

### 1.3 The vault file — `identity.olbk`

JSON for the same reason `store.json` is JSON: auditable by eye, versioned, and
mechanical to migrate. Binary fields are base64url, unpadded.

```json
{
  "v": 2,
  "mode": "passphrase",
  "kdf":  {"alg": "argon2id", "m": 65536, "t": 3, "p": 1, "salt": "<16 B>"},
  "wrap": {"alg": "chacha20poly1305", "nonce": "<12 B>", "ct": "<48 B>"},
  "pub":  {"sig_pk": "<32 B>", "seal_pk": "<1184 B>", "fp": "9C4F2B71D0A85E3C11F9E21A"},
  "created": 1761500000
}
```

- `KEK = Argon2id(passphrase, salt, m, t, p) → 32 B`.
- `ct = ChaCha20-Poly1305(KEK, nonce, ms, aad = canon(v, mode, kdf, pub))`.
  Binding the public half as AAD stops an attacker swapping in their own
  `sig_pk` while leaving the sealed `ms` in place.
- `pub` is cached so the UI can show a fingerprint and the identity can be
  *recognised* while locked, without unsealing.
- KDF parameters live in the file, so they can be raised later without a format
  change. Re-seal on unlock when the stored params are below the current floor.
- Mode 0600, write-then-rename, same as `store.json`.

### 1.4 Unlock modes and the threat model

Three modes. Exactly one is a real protection.

| Mode | KEK source | For |
| --- | --- | --- |
| `passphrase` | Argon2id over a user passphrase | **default** for any node holding content keys |
| `device` | 32 B random, stored in the OS keyring, file holds only a key id | opt-in convenience, Phase 2 (§1.5) |
| `none` | `KEK = Expand("olb-unprotected-v1", 32)` — a constant, i.e. no secret | replica nodes and `OLB_DEV=1` only |

**A passphrase is genuinely needed, and there is no way around it.** The
alternatives all reduce to a secret sitting on the same disk as the thing it
protects:

| Attacker | `passphrase` mode | `none` mode (and today's `identity.key`) |
| --- | --- | --- |
| One-shot read of the store dir (stolen backup, Time Machine, a synced folder, an exfil script) | Cannot sign as you. Cannot unseal invites or project keys. Can read everything `store.json` holds in the clear — project titles, note bodies, member names, fingerprints, the commit log, CIDs. | Becomes you, permanently and silently. Can sign commits as you, unseal every invite addressed to you, and read every project key you hold. |
| Write access to the store dir | Can destroy or replace the vault (denial of service, and you lose the identity if there is no recovery code). Cannot forge a past commit — peers hold signatures under the real key. | Same, plus everything above. |
| Code running as you while OLB is unlocked (malware, a debugger, a hostile MCP server) | Everything. `ms` is in `olb_core`'s memory by definition. | Everything. |
| A peer on the network | Sees ciphertext, commit metadata and topic traffic. No key material. | Same. |

So the honest claim is narrow and worth putting on the Settings screen in these
words: *a passphrase protects your key against someone who copies your files.
It does not protect against someone who is running code on your machine while
Open Labbook is open.*

Why not the OS keyring as the default. On macOS the keychain ACL that permits
silent access is bound to the caller's code signature; a nix-built, unsigned
`olb_core` has no stable signing identity, so the keychain re-prompts on every
rebuild — the user types a passphrase either way, just the OS's one, with worse
copy and no portability. On Linux `secret-service` needs a running keyring
daemon, which a headless replica node does not have. Keyring stays an opt-in
"remember on this device" that stores the KEK after the user has already set a
passphrase, never the only protection.

`none` mode is defensible for exactly one role: a replica node holds **no
content keys** (`docs/04` §8), so its vault protects only its own pin grant.
Settings must show the mode, and show it amber when it is not `passphrase`.

Rate limiting: 5 failed unlocks → 30 s backoff, doubling to 5 min. Say plainly
that this only slows someone typing at your keyboard; an attacker with the file
attacks Argon2id offline at their own pace, which is why the Argon2 parameters,
not the counter, are the defence.

### 1.5 UX

**First run — M2 "Create your Open Labbook identity"** (new modal, not yet
designed; build it on the `Modal` shell M1 uses).
1. Name, ORCID (optional). Both labelled "as you set them · not verified by
   Open Labbook", matching M1 step 1's copy.
2. Passphrase + confirm. Minimum 12 characters, no composition rules, no
   strength meter that lies — one line: "If you forget it, your recovery code is
   the only way back." No new dependency for scoring.
3. Recovery code, shown once, with "Copy" and a mandatory checkbox "I have
   written this down". Re-showable from Settings while unlocked.

**Every launch — M3 "Unlock Open Labbook"**. One passphrase field, `Unlock`
primary, "Use recovery code instead" as a text link. Wrong passphrase: amber
"That passphrase does not match the key on this computer." The app shows nothing
behind it — not the board, not a read-only board. `store.json` is readable
without the key, so a read-only view is technically possible and would be a lie
about what state the app is in.

Because `launch` wipes the profile every time, this modal appears on **every**
launch. That is the real cost of the guarantee. Development escape hatch:
`OLB_PASSPHRASE` is honoured only when `OLB_DEV=1`, and the Settings screen
shows "Developer unlock active" in amber when it is.

**Settings** gains: fingerprint (click to copy, full value in tooltip), "Key
protection: Passphrase / Not protected", `Change passphrase`, `Show recovery
code`, `Rotate my identity key` (a new identity — explain that it does not
inherit membership), and the key file path.

**Core API** (extends `docs/06` §Session and identity; the UI reaches these the
only way it can, QML → `.rep` SLOT → `olb_ui` C++ → `modules().olb_core`):

| Method | In | Out |
| --- | --- | --- |
| `identity.state` | `{}` | `{status: "absent"\|"locked"\|"unlocked", mode, fingerprint?}` |
| `identity.create` | `{name, orcid?, passphrase}` | `{olbId, fingerprint, recoveryCode}` |
| `identity.unlock` | `{passphrase}` | `{ok}` / error `bad_passphrase`, `backoff` |
| `identity.lock` | `{}` | `{ok}` |
| `identity.changePassphrase` | `{old, new}` | `{ok}` |
| `identity.restore` | `{recoveryCode, passphrase}` | `{olbId, fingerprint}` |

**Every other `olb_core` method returns error `locked` while the vault is
locked**, including `board`, `projects` and `notes`. That is the contract line
that keeps the lock honest.

The passphrase crosses the module IPC seam as a JSON string. The socket is a
unix socket under `/tmp/lgs-<profile>` owned by the user, so this adds no new
attacker — but it means the passphrase must never be logged, must be zeroized
on both sides of the seam, and must not appear in any call-tracing debug output
`olb_ui` grows later. Note it in the backend, not just here.

### 1.6 Migration from `identity.key`

On start, in order:
1. `identity.olbk` exists → `locked`; prompt M3.
2. Else `identity.key` exists and is 32 bytes → **adopt it as `ms`**, derive the
   sealing and inbox keys from it, and run M2 with the name/ORCID already in
   `store.json` and the passphrase step framed as "Protect the key this computer
   already has". The fingerprint does not change, so the existing commit log
   stays verifiable. Delete `identity.key` only after `identity.olbk` is
   fsynced and renamed.
3. Else → `absent`; run M2 fresh.

Commits written before `load_or_create_key()` landed are signed by keys that no
longer exist anywhere. They cannot be re-signed — history is append-only. Mark
them `author_unknown` on load and show them in History as "signed by an earlier
session of this computer"; do not count them as verified. Do not invent an
"adopt history" commit: a signature you cannot check is not improved by a newer
signature asserting you own it.

### 1.7 What this does not protect — state it in the UI

Records, notes and the commit log sit in `store.json` **in the clear** today.
Sealing the key stops impersonation; it does not make the laptop a safe. Until
the store DEK of §1.2 is wired (Phase 1b task 2, "encrypted at rest"), "Private
project" means private *to non-members and on the wire*, not encrypted on your
own disk. The privacy copy on S8 step 4 should not be read as more than that,
and if a user would read it as more, the copy needs changing — raise it rather
than quietly shipping the gap.

---

## 2. The Open Labbook ID

`docs/04` §2.1's shape, with the signature tightened.

```
olb1:<base64url-unpadded( CBOR map {
  v: 1,
  name: tstr,                  // self-declared, unverified
  orcid: tstr | null,          // self-declared, unverified
  sig_pk: bstr .size 32,       // Ed25519
  seal_pk: bstr .size 1184,    // ML-KEM-768 encapsulation key
  inbox: bstr .size 16,
  created: uint,               // unix seconds
  self_sig: bstr .size 64
} )>
```

~1330 bytes of CBOR, ~1780 characters of text.

**`self_sig` is over a canonical concatenation, not over re-encoded CBOR:**

```
"olb-id-v1" ‖ u8(v) ‖ L(name) ‖ L(orcid) ‖ sig_pk ‖ seal_pk ‖ inbox ‖ u64be(created)
L(x) = u32be(len(x)) ‖ x      // orcid absent => len 0
```

A verifier that must re-serialise CBOR byte-for-byte to check a signature is one
encoder upgrade away from rejecting every valid ID in the wild. Sign bytes you
construct yourself.

**Fingerprint** = first 12 bytes of `SHA-256("olb-fp-v1" ‖ sig_pk ‖ seal_pk)`,
shown as 6 groups of 4 uppercase hex: `9C4F 2B71 D0A8 · 5E3C 11F9 E21A`. It
covers **both** keys, so an attacker cannot keep a verified signing key and
substitute their own sealing key to receive the project key.

**Verification, in order** (`identity.parse`; any failure → error `invalid`, one
message, no detail that helps an attacker tune input):
1. `olb1:` prefix; base64url decodes; CBOR parses as a map with exactly these keys.
2. `v == 1`.
3. `sig_pk` is 32 bytes and a valid, non-small-order Ed25519 point (`ed25519-dalek`'s
   `VerifyingKey::from_bytes` plus its strict checks).
4. `seal_pk` is 1184 bytes and passes the ML-KEM-768 encapsulation-key check.
5. `inbox` is 16 bytes; `created` is within ±1 day of now in the future and not
   before 2026.
6. `self_sig` verifies over the canonical bytes under `sig_pk`.
7. `name` and `orcid` are untrusted display strings: length-capped (name 80,
   ORCID 19), control characters and bidi overrides stripped, never interpolated
   into QML without escaping. ORCID is checked for format and its ISO 7064
   checksum only — a well-formed ORCID is not a verified one.

**Display truncation.** `olb1:` + first 20 characters of the body + `…` + last 4.
Note that `inviteDemo.olbId` in the fixture (`olb1:9c4f2b71d0a85e3c11f9…e21a`) is
the *fingerprint hex*, not a prefix of a real base64url body — a fixture
shorthand, not the format. Users compare the **fingerprint**, never the ID; the
ID box is for paste and copy only.

---

## 3. Invite flow

### 3.1 Envelope

```
InviteEnvelope {                       // public header, visible to anyone on the bucket topic
  v: 1, invite_id: 16 B, project_id, to_fp: 12 B,
  from_sig_pk: 32 B, epoch: uint, expiry: uint,
  kem_ct: 1088 B,                      // ML-KEM-768 ciphertext
  nonce: 12 B, ct: bstr,               // sealed payload
  outer_sig: 64 B                      // Ed25519(from) over canon(header ‖ kem_ct ‖ nonce ‖ ct)
}

sealed payload {
  project_name, inviter_name, role, idea_ids, note,
  project_key: {epoch, key: 32 B},
  idea_keys: [{idea_id, epoch, key: 32 B}],
  sync_topic, head_commit, snapshot_cid,
  inner_sig: 64 B
}
```

Sealing:

```
(kem_ct, ss) = MLKEM768.Encaps(invitee.seal_pk)
prk   = HKDF-Extract(salt = invite_id ‖ to_fp, ikm = ss)
key   = Expand(prk, "olb-invite-aead-v1", 32)
nonce = Expand(prk, "olb-invite-nonce-v1", 12)
ct    = ChaCha20-Poly1305(key, nonce, payload, aad = canon(header))
inner_sig = Ed25519(from, "olb-invite-inner-v1" ‖ invite_id ‖ to_fp ‖ canon(payload without inner_sig))
```

`inner_sig` is the part `docs/04` leaves out and it matters: with only an outer
signature over the ciphertext, anyone who obtains a decrypted payload can
re-seal it to a third party and sign it as themselves, and the recipient cannot
tell a genuine invite from a relayed one. Binding the plaintext to `to_fp` makes
that detectable. The invitee **must** check `inner_sig`, that `to_fp` is their
own fingerprint, and that `from_sig_pk` matches the fingerprint the human
confirmed out of band.

Also: never derive the nonce from a counter here. One envelope, one KEM
encapsulation, one fresh `ss`, one nonce — no reuse window exists, and it should
stay that way by construction.

### 3.2 The short human code

`code = "OLB-" + Crockford-base32(10 random bytes)` in groups of 4 —
`OLB-7Q4M-K2XD-93RB`, 80 bits.

Say what it is for, because the design copy is right and the instinct is wrong:
**the code is a rendezvous token, not a key and not an authorization.** It
carries no project data (M1's copy: "The code holds no project data. It only
lets their node find yours."). What it buys:

- the newcomer can find the lead's join topic (`/olb/1/join-<H(code)[0] mod 16>/proto`)
  without knowing the lead's inbox or project id;
- `K = HKDF(code, "olb-join-v1")` encrypts the newcomer's OLB ID inside the
  `JoinRequest`, so a passive observer of a public bucket topic does not learn
  who is trying to join what;
- `tag = SHA-256(code)` lets the lead match a request to a code in constant time
  without the code appearing on the wire.

What it does **not** buy: access. Nothing is sealed to the newcomer until a
human approves, after comparing a fingerprint. Holding the code lets you place a
request and nothing else. Rate-limit 5 requests per code per hour, single use by
default, 24 h / 48 h (default) / 7 d expiry, "Make a new code" revokes the old
one (`invites.revoke`).

### 3.3 How the project key reaches the invitee

Invite by ID: inside the envelope, straight to the invitee's inbox topic. Invite
by code: the code path never carries keys — the lead's approval produces exactly
the same `InviteEnvelope` as the ID path, sent to the inbox in the OLB ID the
`JoinRequest` supplied. One sealing path, two ways of discovering a public key.

On accept the invitee publishes `InviteAccept {invite_id, their sig_pk, sig}`
to the project topic; the inviter verifies it and writes the `member_add` commit
(§4) with that signature attached as proof of consent.

### 3.4 States, and how they map to the four designed steps

One enum for both paths. `invites.list()` returns it; S7's Pending invites panel
and M1 step 4 render it.

| State | Meaning | Where it shows |
| --- | --- | --- |
| `draft` | M1 open, nothing sealed | M1 steps 1–2 only, never in `invites.list()` |
| `sealed` | envelope built, keys sealed | M1 step 4 row 1 "Project key sealed to their key" → Done |
| `sent` | handed to Delivery | step 4 row 2 "Handed to Logos Delivery" → Done; S7 amber "Not yet received by their node" + Resend |
| `received` | invitee's node returned a signed receipt | step 4 row 3 → Done |
| `accepted` | `InviteAccept` verified, `member_add` committed | step 4 row 4 "Accepted, key installed" |
| `declined` | signed decline | S7 row, with Remove |
| `expired` | past `expiry` (default 14 days) | S7 row, with Resend as a new invite |
| `revoked` | lead revoked | S7 row |
| `code_active` | code live, no redemption | step 4 code rows 1 → Done |
| `redeemed` | `JoinRequest` matched and decrypted | step 4 code row 2; S7 violet "Waiting for your approval" + Review |

The fixture's `pendingInvites[].state` uses `sent` and `awaiting_approval`;
`awaiting_approval` is this table's `redeemed`. Keep the fixture, map it in the
adapter, and use these names on the wire.

Design-step mapping: `InviteStep2` (role and access) and `InviteStep3` (review
and send) are both `draft` — nothing is sealed until the step-3 primary is
pressed, so Esc at step 2 or 3 leaks nothing and needs no revoke. `InviteStep4`
is the first screen backed by a real record, and "Review" on a `redeemed` code
reopens step 3 with the requester's fingerprint and an `Approve` primary.

---

## 4. Membership without Chat groups

Chat groups cannot remove members (`CLAUDE.md` 6), and a Chat address changes
every launch (`CLAUDE.md` 5), so membership is neither a Chat group nor a list of
addresses. **Membership is a signed record series in the project's own commit
DAG**, keyed by Ed25519 `sig_pk`. The current member list is a replay of that
series, never a stored table — a stored table is a thing that can disagree with
the history that justifies it.

Record kinds (signed commits, `docs/05`):

| Kind | Payload |
| --- | --- |
| `project_genesis` | `{project_id, creator_sig_pk, creator_seal_fp, created}` — self-signed; the creator is the founding Lead |
| `member_add` | `{sig_pk, seal_fp, name_at_add, role, idea_ids, invite_id, accept_sig}` |
| `member_remove` | `{sig_pk, reason?}` |
| `role_change` | `{sig_pk, role}` |
| `access_grant` / `access_revoke` | `{sig_pk, idea_ids}` |
| `epoch_rotate` | `{epoch, reason, for_ideas, grants: [{to_fp, kem_ct, nonce, ct}]}` |
| `replica_grant` / `replica_revoke` | `{node_sig_pk, scope}` |

### Who may do what

Validated by every member's core against the membership state at the commit's
parent — not against local state, or a reordered DAG changes the answer.

- Only a **Lead** may `member_add`, `member_remove`, `role_change`,
  `access_grant`, `access_revoke`, `replica_grant`, `replica_revoke`.
- A `member_add` is invalid unless `accept_sig` verifies under the added
  `sig_pk` over `"olb-accept-v1" ‖ invite_id ‖ project_id`. **Consent is
  structural**: without it a lead could name a researcher as a member, and
  therefore as an author on a publication, who never agreed. This is the single
  most important authority rule in the file.
- Nobody may add themselves; the only exception is `project_genesis`.
- Any member may `member_remove` **their own** `sig_pk` (leave), which forces
  rotation exactly as a removal does.
- A project must always have ≥ 1 Lead: a `member_remove` or `role_change` that
  would empty the Lead set is invalid. Prevents a project with no one able to
  administer it, and prevents a removal race from orphaning it.
- Role enforcement on content commits: a Reviewer's record edits are rejected,
  a Contributor's edits are accepted only in ideas their access list covers
  (`docs/04` §6). Rejected commits are not silently dropped — History shows
  "rejected · not from a permitted signer", because a peer sending commits it
  is not allowed to send is the signal that matters.

### Where this is honestly weak

Two Leads on either side of a partition can each write a valid
`member_remove` for the other. The DAG merges and both are valid against their
own parent. Local rule: membership records for the same subject are ordered by
(DAG depth, then commit id) and the first valid one wins; the loser is marked
`superseded` and shown in History. That makes the fork **detectable and
deterministic**, not prevented. The LEZ anchor of the membership root is what
makes a fork visible to everyone rather than only to whoever merged — which is
the real argument for anchoring membership roots, not just record hashes.

And a malicious lead can simply never relay your removal to you. Membership is
only as consistent as the sync that carries it; a node that is cut off believes
the last thing it saw. Nothing in a peer-to-peer design fixes that, and the UI
should not imply membership is globally instantaneous.

---

## 5. Key rotation

### Key hierarchy, corrected

`docs/04` §6 draws `project_key → wraps → idea_key[...]`. **That defeats
per-idea access control**: a Reviewer holding the project key could unwrap every
idea key, so "Selected ideas" would be decoration. Correct hierarchy:

- `project_key[epoch]` — 32 random bytes, protects project-level metadata
  (title, stages, member list ciphertext, commit envelopes). Every member has it.
- `idea_key[idea_id][epoch]` — 32 **independent** random bytes per idea.
  Not derived from and not wrapped by `project_key`. Sealed per member, per
  idea, to their ML-KEM key.

Every encrypted record and file carries `{key_id, epoch}` in its header, so a
reader knows which key to try without guessing.

### When rotation is forced

| Trigger | Rotate |
| --- | --- |
| `member_remove` (or a member leaves) | `project_key`, **and** every `idea_key` that member held |
| `access_revoke` on an idea | that `idea_key` only |
| `role_change` that narrows access | the affected `idea_key`s |
| "Rotate keys now" in S7 (suspected compromise) | everything |
| `member_add` | **nothing** — adding needs no new epoch |

Ideas the departing member never had access to are not rotated. There is nothing
to protect and the cost is per-member sealing work.

### The rotation rule

**New content gets a new epoch; existing content is never re-encrypted.**

Re-encrypting history would be worse than useless. It protects nothing — the
removed member already holds the plaintext, or the old key, or both, and
bytes they have already copied do not become unreadable. And it would be
destructive: re-encryption changes content addresses, which breaks every CID,
every snapshot, every Merkle root already anchored on the LEZ, and the
append-only guarantee in `CLAUDE.md`. Unbounded cost, zero benefit.

### Delivering the new epoch

The `epoch_rotate` commit carries one sealed grant per remaining member,
sealed with the same ML-KEM-768 → HKDF → ChaCha20-Poly1305 construction as an
invite (`salt = epoch_id ‖ to_fp`, info `"olb-epoch-aead-v1"`).

This deliberately departs from `docs/04` §7 step 3, which sends new keys "via
inbox". Delivery Store is temporary (`CLAUDE.md` 4): a member offline for a week
during a rotation would lose the key and silently stop being able to read the
project. Putting grants **in the commit history** means they arrive with any
snapshot from Storage, as durably as the content they unlock.

`projectKey.status` returns `{version, rotatedAt, pendingGrants}`; S7's panel
shows "Version 3 · last rotated 2 Oct", and a member who has not yet picked up a
grant is visible rather than mysteriously stale.

### What a removed member keeps — say it out loud

Removal cannot retroactively un-share anything. Concretely, a removed member
keeps, forever:

- every record and file at epoch ≤ removal, decrypted or decryptable;
- every CID they pinned, and any local copy they made;
- the member list, history and fingerprints as of removal;
- metadata after removal: the project's bucket topic is derived from
  `project_id` and cannot change, so they can keep observing commit volume and
  timing, though not content.

They lose read access to everything at epoch n+1 and later, and their commits
are rejected by every honest node. S7's existing copy — "They keep what they
already had, but cannot read anything new." — is exactly right and should not be
softened. If a user needs someone to *not have* something, rotation is not the
tool; the data was already shared.

---

## 6. Tests

Required by `CLAUDE.md` ("invite sealing/unsealing, key rotation"). Unit tests
in `olb_core/rust-lib`; the two-profile cases are the Phase 1c acceptance test.

**Vault and durable key**
1. Create → lock → unlock with the right passphrase yields a byte-identical
   `ms`, `sig_pk` and fingerprint.
2. Wrong passphrase → `bad_passphrase`, and `ms` is not partially written to
   memory the caller can reach.
3. Flipping any byte of `wrap.ct` → AEAD failure, not a wrong key.
4. Editing `pub.sig_pk` in the file → AEAD failure (the AAD binding).
5. Unlock → derive → re-create from the recovery code in a clean directory →
   same fingerprint and same `seal_pk`.
6. Simulated relaunch (new process, same store dir): fingerprint and the
   `author` of a new commit match the previous run. **This is the test that
   closes the R6 gap.**
7. Migration: a legacy 32-byte `identity.key` adopted into a vault keeps the
   same fingerprint, and the pre-existing commit log still verifies.
8. Crash mid-write (write temp, do not rename) leaves the old vault intact.
9. KDF upgrade: a vault with below-floor Argon2 params unlocks and is re-sealed
   at the current floor.
10. Every non-identity method returns `locked` before unlock.

**OLB ID**
11. Round trip: generate → encode → parse → fields and fingerprint match.
12. Rejects: bad prefix, bad base64url, truncated CBOR, `v != 1`, 31-byte
    `sig_pk`, 1183-byte `seal_pk`, small-order `sig_pk`, bad `self_sig`, a
    `created` far in the future.
13. Tampering with `name` invalidates `self_sig` (the canonical-bytes signature
    actually covers it).
14. Substituting `seal_pk` while keeping a valid `sig_pk` changes the
    fingerprint — the attack the two-key fingerprint exists to stop.
15. Name with control characters, RTL overrides and 10 kB of text is truncated
    and sanitised, not rejected into a crash.

**Invite sealing and unsealing**
16. Seal to carol's `seal_pk` → carol unseals; project key and idea keys match
    byte-for-byte.
17. Bob (not the addressee) cannot unseal: KEM decapsulation yields a different
    secret and AEAD fails.
18. Flipping one byte of `ct`, of `kem_ct`, or of any header field in the AAD →
    decrypt fails.
19. `outer_sig` forged with a different key → rejected before decryption is
    attempted.
20. Relay attack: take a valid payload, re-seal to dave, sign as mallory →
    dave's `inner_sig` / `to_fp` check rejects it.
21. Replay: the same `invite_id` presented twice → second is rejected.
22. Expired envelope (`expiry` in the past) → rejected with `expired`.
23. Reviewer invite with 2 of 5 idea keys: the payload contains exactly those
    two, and the other three are not derivable from `project_key`.
24. Code path: `JoinRequest` tag matches in constant time, decrypts under
    `HKDF(code)`, and a one-character-wrong code matches nothing; the 6th
    request in an hour is rate-limited.

**Membership**
25. Replaying genesis + adds + role changes + removes gives the expected member
    list, and replaying in a different DAG-valid order gives the same list.
26. `member_add` without a valid `accept_sig` is rejected.
27. A Contributor's `member_add` is rejected; a Reviewer's record edit is
    rejected and surfaces as "rejected · not from a permitted signer".
28. A `member_remove` that would empty the Lead set is rejected.
29. Self-removal by a Contributor is accepted and forces rotation.
30. Two concurrent conflicting `member_remove`s resolve identically on two nodes
    and mark the loser `superseded`.

**Key rotation**
31. Remove a member → `epoch` increments, a new `project_key` and new keys for
    exactly the ideas they held, and unaffected idea keys are unchanged.
32. The removed member's old keys still decrypt pre-rotation records (the
    honest-limits test) and fail on post-rotation records.
33. A remaining member unseals their `epoch_rotate` grant and reads new content.
34. A member offline across the rotation reads the grant from a Storage snapshot
    with no inbox message at all.
35. `access_revoke` on one idea rotates that idea key only.
36. `member_add` writes no `epoch_rotate`.
37. `projectKey.status` reports version, timestamp and `pendingGrants` correctly.

**Two profiles (Phase 1c acceptance, `docs/07`)**
38. alice invites carol by OLB ID as Reviewer with 2 of 5 ideas; carol accepts;
    carol reads only those two; a forged edit from carol is rejected on bob.
39. alice invites dave by code; dave redeems; alice approves after comparing
    fingerprints; alice removes dave; the key version increments and dave cannot
    decrypt a new entry.
40. Both profiles relaunched: fingerprints, membership and the commit log all
    survive, and each node needs its passphrase once.

---

## 7. Where `docs/04` is underspecified, and where the guarantee is weaker than it reads

1. **§6's key hierarchy is wrong as drawn.** `project_key` wrapping the idea
   keys makes per-idea access meaningless. §5 above replaces it with independent
   idea keys. This is a correctness bug, not a refinement.
2. **§4 signs only the ciphertext.** No binding between the plaintext and the
   recipient, so a decrypted invite can be relayed and re-signed. Fixed with
   `inner_sig` + `to_fp` (§3.1).
3. **§7 step 3 delivers rotated keys over the inbox.** Delivery Store is
   temporary; a member offline during a rotation loses the key. Grants belong in
   the commit history (§5).
4. **The `received` state claims more than Delivery can prove.** There is no
   sender-side delivery proof; `received` can only come from a receipt the
   invitee's node chooses to send. Until it arrives, "Not yet received by their
   node" is indistinguishable from "they are offline" and from "we failed to
   publish". Keep the amber copy, do not add a claim behind it.
5. **Fingerprint comparison is the entire trust anchor, and it is
   self-attested.** M1's checkbox "I compared this fingerprint with {name} in
   person or on a call" is the only thing standing between OLB and a
   man-in-the-middle who substitutes their own OLB ID. Nothing verifies it. The
   security of the invite flow is exactly the probability that the lead actually
   made the call.
6. **"ORCID linked" in S7 reads as verified and is not.** M1 step 1 is honest
   ("Name and ORCID as they set them · not verified by Open Labbook"); the S7
   badge and `inviteDemo.parsed.orcidClaimed` are not. There is no ORCID OAuth in
   Phase 1c, so the badge should read "ORCID claimed". Copy is final per
   `CLAUDE.md`, so this needs a decision rather than a quiet change — flagging it.
7. **Records are plaintext at rest** until the store DEK lands (§1.7). "Private
   project" is a statement about the network and non-members, not about your
   disk.
8. **The fixture's `inviteDemo.olbId` is fingerprint hex, not a real ID prefix.**
   Harmless as a fixture, misleading as a format reference (§2).
9. **16 inbox buckets is thin k-anonymity.** With a handful of testnet users, a
   bucket identifies a person almost as well as an address would. The property
   `docs/04` §3 claims arrives only at scale; it is not a guarantee today.
10. **A replica grant is unforgeable but unenforceable.** Nothing stops a node
    pinning project ciphertext without a grant, and nothing stops a removed
    member continuing to do so. The grant records who is *supposed* to hold a
    copy, which is what the S7 copy-count UI needs; it is not access control.

---

## 8. Crates

Audited or formally verified only (`CLAUDE.md`). Pin the exact versions with
`cargo add` and check crates.io — the versions below are what to aim for, not a
substitute for resolving them. `ed25519-dalek 2` holds `rand` at 0.8 via
`rand_core 0.6`; do not bump `rand` past that without checking.

| Crate | Version | Use | Note |
| --- | --- | --- | --- |
| `ed25519-dalek` | 2.2 | signing | already in `Cargo.toml`; use the strict verification API |
| `libcrux-ml-kem` | 0.0.3 | ML-KEM-768 | **preferred**: formally verified (hax/F*), the implementation shipping in Firefox/NSS |
| `ml-kem` | 0.2 | ML-KEM-768 fallback | pure-Rust RustCrypto, self-described as unaudited; behind a feature flag, for a platform libcrux will not build on |
| `argon2` | 0.5 | passphrase → KEK | Argon2id, `m=65536 t=3 p=1` floor, params stored in the vault |
| `chacha20poly1305` | 0.10 | vault wrap, invite and epoch seals | one key, one nonce, always |
| `hkdf` | 0.12 | all key derivation | every call gets a distinct `info` label |
| `sha2` | 0.10 | fingerprints, commit ids, code tags | already in `Cargo.toml` |
| `subtle` | 2.6 | constant-time tag and MAC compare | `docs/04` §9 |
| `zeroize` | 1.8 | wipe `ms`, KEK, passphrases | `ZeroizeOnDrop` on every secret struct |
| `ciborium` | 0.2 | CBOR for the OLB ID and envelopes | maintained; `serde_cbor` is not |
| `base64` | 0.22 | base64url-unpadded | — |
| `keyring` | 3.x | optional `device` mode | Phase 2, opt-in only (§1.4) |
