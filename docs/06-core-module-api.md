# 06 — `olb_core` API (proposed)

> **Corrected 25 Oct 2026 against the pinned source.** The call model below was
> written against `logos.callModule("olb_core", "<method>", [args])`, **which does
> not exist** on the `ui-qml-backend` path. The real chain has three layers:
>
> ```
> QML  --QtRO-->  olb_ui C++ backend  --LogosAPI-->  olb_core (Rust)
> ```
>
> - QML calls a `.rep` SLOT on `logos.module("olb_ui")`; the reply is asynchronous
>   and arrives through `logos.watch(pending, onValue, onError)`.
> - `.rep` PROPs auto-sync to QML over QtRO — that is how events reach the view,
>   with no polling. Lists cross as `QAbstractItemModel`, read as
>   `logos.model("olb_ui", "<prop>")`.
> - The `olb_ui` backend calls `modules().olb_core.<method>(...)` (synchronous,
>   returns `Result`) and arms event subscriptions in `onContextReady()`.
>
> So every method in this file needs **two** declarations: the `olb_core` contract
> entry, and an `olb_ui` `.rep` SLOT or PROP that relays it. Full evidence in
> `docs/08` R2 and R3; the reference implementation is `logos-chat-ui`.

## Conventions

- `olb_core` declares its contract in a **committed `olb_core.lidl`** with record
  types, arrays and optional fields (as `chat_module` does), not as JSON strings.
  The JSON-envelope convention below survives only where a payload is genuinely
  open-ended; prefer a typed record wherever the shape is known.
- The legacy envelope, where still used: `{"ok": true, "data": …}` or
  `{"ok": false, "error": {"code": "…", "message": "…"}}`.
- Long operations (uploads, anchoring, sending) return immediately with `{"ok": true, "data": {"job": "<id>"}}` and report progress through events.
- Ids are strings. Times are RFC 3339. Amounts are strings of integer base units.
- Error codes: `not_found`, `forbidden`, `invalid`, `conflict`, `offline`, `chain_error`, `storage_error`, `internal`.

## Methods by area

### Session and identity
| Method | Input | Output |
| --- | --- | --- |
| `session.status` | `{}` | `{node: "online|offline", network: "testnet v0.3", peersOnline, peersTotal, fixtures: bool}` |
| `identity.me` | `{}` | `{name, orcid, fingerprint, olbId}` |
| `identity.update` | `{name?, orcid?}` | `{olbId}` (re-signed) |
| `identity.parse` | `{olbId}` | `{valid, name, orcid, fingerprint, online}` or error `invalid` |

### Projects and board
| Method | Input | Output |
| --- | --- | --- |
| `project.list` | `{}` | `[{id, title, privacy, ideaCount}]` |
| `project.create` | NewProject payload (below) | `{projectId, inviteJobs[]}` |
| `project.board` | `{projectId}` | `{title, privacy, badges[], status:{sync, copies, history}, ideas[], activity[]}` |
| `idea.create` | `{projectId, title, summary, stages[]}` | `{ideaId, commit}` |

NewProject payload:
```json
{ "title": "…", "summary": "…", "description": "…", "startDate": "2026-11-01", "duration": "6-12m",
  "licence": "CC-BY-4.0", "field": "biochemistry", "keywords": ["lipase", "thermostability"],
  "teamSize": "2-5", "members": [{"olbId": "olb1:…", "role": "contributor"}],
  "stages": ["idea","hypothesis","funding","experiments","data","publishing"],
  "privacy": "open|private|invite_only",
  "anchoring": "every|batched|daily|milestones", "mergeApprovals": 1, "replica": "default|none",
  "allowBranches": true, "autoTags": true,
  "references": [{"type": "paper|dataset|protocol|preprint|web|olb", "identifier": "…", "title": "…"}],
  "grant": "…" }
```

### Records and notebook
| Method | Input | Output |
| --- | --- | --- |
| `record.get` | `{id, version?}` | record + `{signatures[], anchor, children[]}` |
| `record.versions` | `{id}` | `[{version, author, time, commit}]` |
| `record.saveDraft` | `{id?, type, ideaId, fields}` | `{id, version: "draft"}` |
| `record.commit` | `{id, message}` | `{commit, version}` |
| `record.flagNegative` | `{id, value}` | `{commit}` |
| `agent.suggestions` | `{recordId}` | `[{id, label, before, after, confidence, source}]` |
| `agent.apply` / `agent.reject` | `{suggestionId}` | `{commit?}` |

### Files
| Method | Input | Output |
| --- | --- | --- |
| `files.add` | `{recordId, path}` (path chosen by a native file dialog in core, not QML) | `{job}` → `fileAdded {cid, name, size}` |
| `files.status` | `{cids[]}` | `[{cid, copies, onThisNodeOnly}]` — no network replica count exists; `copies` is counted from member/replica acknowledgements over Delivery. `exists(cid)` is local-only. |
| `files.pin` | `{cid, target: "replica"}` | `{job}` — **Storage has no pin primitive.** Implement as `fetch(cid, …)` + `setAdvertise(cid, true)` plus a re-fetch schedule, because cached blocks expire at `block-ttl` (default 30d). See `docs/08` R4. |

### Hypotheses
| Method | Input | Output |
| --- | --- | --- |
| `hypothesis.previewCommitment` | `{id}` | `{fingerprint}` |
| `hypothesis.commit` | `{id, shareSealedWithReviewers}` | `{job}` → `anchorStatusChanged` |
| `hypothesis.verifyReveal` | `{id}` | `{matches, committedAt, batch}` |

### Funding (mirrors `olb_funding`)
`funding.get {proposalId}`, `funding.approve {milestoneId}`, `funding.requestEvidence {milestoneId, note}`, `funding.contribute {proposalId, amount, private}`.

### Publishing
`publish.prepare {ideaId}` → `{chain[], reveals[], credits[], blockers[]}`; `publish.execute {ideaId, reveals[], licence, openReview, exportRoCrate, inviteReproduction}` → `{job}`.

### Version control
| Method | Input | Output |
| --- | --- | --- |
| `vcs.log` | `{projectId, ideaId?, branch, limit, before?}` | rows (format in `docs/05-version-control.md` §6) |
| `vcs.diff` | `{commitId}` | `{parent, signer, changes[{record, field, before, after}], files{added[], removed[], changed[]}}` |
| `vcs.branches` / `vcs.tags` / `vcs.mergeRequests` | `{projectId}` | lists |
| `vcs.branch` | `{name, fromCommit}` | `{branch}` |
| `vcs.tag` | `{name, commitId}` | `{tag}` |
| `vcs.restore` | `{commitId, recordId?}` | `{commit}` |
| `vcs.openMergeRequest` / `vcs.approveMerge` / `vcs.merge` | … | … |
| `anchor.status` | `{projectId}` | `{lastBatch, lastAnchoredAt, pending}` |
| `anchor.verify` | `{commitId}` | `{valid, batch, root}` |

### Members and invites
| Method | Input | Output |
| --- | --- | --- |
| `members.list` | `{projectId}` | `[{name, orcidLinked, role, ideaAccess, fingerprint, online, lastSeen}]` |
| `members.setRole` / `members.setAccess` / `members.remove` | … | `{commit}` (remove triggers key rotation job) |
| `invites.createById` | `{projectId, olbId, role, ideaIds|"all", note}` | `{inviteId, job}` |
| `invites.createCode` | `{projectId, role, ideaIds|"all", expiryHours, uses}` | `{code, expiresAt}` |
| `invites.list` | `{projectId}` | `[{id, kind, name?, fingerprint?, state, sentAt, code?}]` |
| `invites.resend` / `invites.revoke` / `invites.approveRequest` | `{inviteId}` | `{job}` |
| `invites.redeem` (invitee side) | `{code}` | `{requestId}` |
| `invites.incoming` / `invites.accept` / `invites.decline` (invitee side) | … | … |
| `replicas.list` / `projectKey.status` | `{projectId}` | … |

### References
`references.resolve {identifier}` → `{type, title, authors, year}` (DOI via Crossref/DataCite **through a core-side HTTP fetch**; confirm core modules may reach the network); `references.importBib {path}`.

## Events (core → UI)

| Event | Payload | Screens |
| --- | --- | --- |
| `syncStatusChanged` | `{peersOnline, peersTotal, state}` | Nav box, S1 |
| `recordChanged` | `{projectId, recordId, version, commit}` | S1, S2, S6 |
| `filesStatusChanged` | `{cid, copies}` | S1, S2, S5 |
| `anchorStatusChanged` | `{projectId, batch, commits[]}` | S1, S2, S3, S6 |
| `jobProgress` | `{job, state, detail}` | M1 step 4, uploads |
| `inviteStatusChanged` | `{inviteId, state}` | S7, M1 |
| `inviteReceived` | `{inviteId, projectName, inviterName, fingerprint}` | Join prompt |
| `fundingChanged` | `{proposalId}` | S4 |

## Fixtures mode

`olb_core` must support `OLB_FIXTURES=1` (env var set in the scaffold profile): all read methods return data from `fixtures/sample-project.json`, writes succeed in memory, events are simulated (for example invite states advance every few seconds). This lets UI work proceed before the backend.
