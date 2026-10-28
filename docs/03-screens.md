# 03 — Screens

> **Amended 26 Oct 2026** (by request, after seeing S1 running). These changes
> supersede the handoff copy where they conflict:
>
> - Nav label "Board" is now **"Public Board"**, and "Notebook" is
>   **"Personal Notebook"**.
> - A **Settings** screen is added (profile name + ORCID, signing-key
>   fingerprint, store location, service status). Not in the original nine.
> - **New project** is a nav destination, not only an action on S1.
> - The Personal Notebook is a new concept: private notes belonging to no
>   project, each **promotable into a project idea entering at a chosen
>   stage** — Idea, Hypothesis, Experiments or Data. Funding and Publish are
>   deliberately not entry points. Promotion is append-only: the note is
>   marked promoted and kept, and a new idea is added to the project.
>
> Screens S2–S7 and M1 below are unchanged and still unbuilt.


Nine surfaces: eight screens and one multi-step modal. Each section lists the design source file in `design/canvas/`, layout, content, data bindings (`olb_core` methods from `docs/06-core-module-api.md`), states and interactions. Exact copy is in the `.dc.html` files; quoted strings below are the important ones.

All screens use `AppShell` with `SideNav` (`docs/02-design-system.md`). The active nav item is noted per screen. Sample content comes from `fixtures/sample-project.json`.

---

## S1 · Project board — `Main.dc.html` (nav: Board)

**Purpose:** see every idea in the project, where each one stands, and what changed recently.

**Layout (top to bottom)**
1. `PageHeader`: meta "Project"; H1 project title; badges "Private to project group" (slate), "5 ideas" (slate), "1 published" (accent). Actions: "New notebook entry" (secondary → S2), **"New project"** (primary → S8).
2. Three `StatusCard`s in a wrapping row:
   - "Live sync" — "3 of 4 members online" — "Edits arrive over Logos Delivery, end-to-end encrypted".
   - "Copies of project files" — "4 copies · 1 always-on replica" — amber sub-line "1 file is only on this laptop · Pin it to the replica" (link → the entry holding that file).
   - "Version history" — "134 signed commits · 3 branches" — "Last anchored on LEZ 12 min ago · Open history" (→ S6).
2b. **`StageTracker`** (added 28 Oct 2026, docs/08 D18): the project's position in its own stages, as a chain — `✓ Idea → 2. Hypothesis → 3. Funding → …`. Green done, violet current (outlined), slate ahead. Below it, when the current stage is unfinished, an amber line naming what it is waiting for: "To finish Hypothesis: Pre-register a hypothesis." It is a **readout, not an input** — there is no control anywhere that sets a stage, because a stage is finished only by doing its work. Data: `project.board` → `stage`, `stageName`, `stageNeeds`, `stagesUsed`; no extra core call, because a ninth synchronous round-trip per refresh is a ninth chance to wedge the UI thread (R21).
3. Section "Ideas" (H2 18/600) with `FilterPills`: All, Idea, Hypothesis, Funding, Experiments, Data, Publish — each with the count of ideas whose **current** stage matches. Published ideas count under Publish.
   **Partly superseded by D18.** The pills assume each idea carries its own stage. Since 28 Oct the stage belongs to the project, so every idea in view shares it and the pills no longer partition anything — "All" and the current stage hold everything, the rest read zero. Left in place and working (cards display the project's stage) rather than ripped out, but this section wants a revisit: either drop the stage pills, or filter on something that still varies per idea (published, lead, branch).
   Alongside the pills: **"Add idea"**, which opens an inline composer (Title, Summary, "Add idea") in a panel directly under them. This is the only way work enters a project other than promoting a personal note, and without it a project created from S8 holds nothing and can never finish its first stage.
4. Grid of `IdeaCard`s: auto-fill columns, min 460 px. Sorted by last update, newest first.
5. Empty state when a filter has no ideas: dashed panel "No ideas are at this stage right now."
6. "Activity" panel: list of events (text with inline links, right-aligned relative time).

**Idea card data** (`project.board` → `ideas[]`): `stage` (1–6 — since D18 the core overwrites this with the **project's** stage on the way out, so a card can never contradict the tracker above it; the stored value records where the idea entered), `published`, `title`, `summary`, `privacy`, `facts` (Hypothesis, Funding, Notebook, Data — four short strings computed by core), `branch`, `headCommit` (short hash), `headMessage`, `commitCount` or MR state, `lead`, `peopleCount`, `updatedAt`, `primaryLink` (which screen the title opens: current stage's screen).

**Stage chip text:** "Stage {n} of {N} · {Stage}" where N is the number of stages the project uses (fixtures use 6); "Published · all {N} stages" when published. Colour: violet; slate when stage 1 and only the author can see it; accent when published.

**Interactions**
- Filter pills toggle local UI state; selected pill uses ink fill. "All" resets.
- Clicking a card title opens the idea at its current stage. The commit strip opens S6 filtered to that idea.
- Live updates: `recordChanged`, `syncStatusChanged`, `anchorStatusChanged` events refresh cards and status cards without a full reload.

**States:** loading (skeleton cards), no ideas yet (illustration-free empty state with "New idea" — to be designed, use a dashed panel with a primary button), offline (Live sync card shows "Offline · changes are saved locally").

> Note: Members were deliberately moved off the board into S7.

---

## S2 · Notebook entry — `Entry.dc.html` (nav: Notebook)

**Purpose:** write and review one experiment record, accept or reject agent suggestions, see provenance and versions.

**Layout**
- `Breadcrumb` Board / Experiments / Run 14.
- Header: H1 "Run 14 · Thermal shift assay, plate 3"; badges "Version 3" (slate), "Draft · not yet shared with group" (amber), "Tests H1" (violet). Actions: "Mark as negative result" (secondary), "Sign and share with group" (primary).
- Two columns.
  - **Main column**
    1. Panel "Purpose and protocol": TextArea Purpose; "Follows" chips (protocol, hypothesis).
    2. Panel "Parameters": `DataTable` Parameter / Value (mono) / Unit / Source ("Instrument file", "Typed by …", "From Protocol v3"); "+ Add parameter".
    3. Panel "Observations and deviations": two TextAreas.
    4. Panel "Files": "Add files" button; `FileRow` per attachment; footer "Files stay encrypted with the project key until you publish them."
  - **Side column**
    1. Panel "Agent suggestions (4)" + meta "Labbook agent · own key · run by Arjun M."; `SuggestionCard`s; footer "Nothing changes until you accept. Accepted edits are credited to the agent, not to you."
    2. Panel "Provenance": Built on (links), Used by (links), Signatures (author signed; approver waiting in amber), On-chain anchor (status + "Verify proof" link for earlier versions).
    3. Panel "Versions": list v3/v2/v1 with author, time, "Compare" (→ S6 diff); footer "Versions are never overwritten."

**Data:** `record.get(id)` (fields, version, parents, children, signatures, anchor), `record.files(id)`, `agent.suggestions(recordId)`, `record.versions(id)`.

**Interactions**
- Edits autosave as a local draft (`record.saveDraft`, debounce 800 ms). Draft badge stays amber until shared.
- "Sign and share with group" → `record.commit(id, message)`; on success badge changes to "Shared · waiting for anchor" then "Anchored in batch N".
- "Mark as negative result" toggles a flag (its own commit); badge "Negative result" (slate) appears.
- Accept/Reject suggestion → `agent.applySuggestion` / `agent.rejectSuggestion`; accepted ones become a commit signed by both agent key and operator.
- "Pin to replica" → `files.pin(cid)`; row updates when copy count changes.

---

## S3 · Pre-register a hypothesis — `Hypothesis.dc.html` (nav: Hypotheses)

**Purpose:** lock a hypothesis and analysis plan before data exists, with only a fingerprint on-chain.

**Layout**
- Breadcrumb Board / Hypotheses / H1. H1 "Pre-register H1". Lede: "Lock the plan before the data arrives. Only a fingerprint goes on-chain; the text stays encrypted with the project key until you reveal it."
- `Stepper` (4 cards): 1 Write (Done), 2 Group review ("2 of 2 approved"), 3 Commit on LEZ (current, violet), 4 Reveal at publication (Later).
- Main panel "The plan": Hypothesis (TextArea 15 px), Predictions fieldset (P1, P2 rows with violet mono labels; "+ Add prediction"), "What would prove it wrong", "Analysis plan", "Grows from" chip, Group review line.
- Side panel (violet 2 px border) "What gets committed": "Public, on LEZ" list (salted fingerprint, time, project) and "Private, stays with the group" list (text, authors, salt); fingerprint preview box (mono `sha256:9f2c41d8…07be41a`); checkboxes "Share a sealed copy with the funding reviewers" and "I understand that edits after this point create a new version, and this one stays on record"; buttons **"Commit to LEZ"** (violetPrimary) and "Save draft"; footnote about the salt being released at publication.

**Data:** `hypothesis.get`, `hypothesis.previewCommitment` (returns fingerprint without committing), `hypothesis.commit`.

**Rules:** "Commit to LEZ" is disabled until the understanding checkbox is ticked and group review is complete (configurable). After commit, the record is tagged `h{n}-preregistered` and the stepper advances to step 4.

---

## S4 · Funding and milestones — `Funding.dc.html` (nav: Funding)

**Purpose:** show escrowed funds and let reviewers release milestones on evidence.

**Layout**
- Breadcrumb; H1 proposal name; badges "Fully funded" (accent), "Tests H1" (violet), "Testnet tokens only" (slate); action "View proposal".
- Four summary cards: Raised (mono 22), Released to the project, Held in escrow, Release rule "2 of 3 reviewers" ("enforced by the escrow program").
- Main column "Milestones": one card per milestone.
  - Released: title + amount, accent badge "Released {date}", approvers, evidence links.
  - Awaiting (amber 2 px border): amber badge "Awaiting approval · 1 of 2 needed", evidence links, approver list with states, buttons **"Approve release"** (primary) and "Ask for more evidence" (secondary). Buttons appear only for reviewers who have not acted.
  - Locked: muted title, slate badge "Locked until M{n-1} is released".
- Side column: "Contributors" (name, public/private, amount mono; private contributor shows "Identity hidden · private LEZ account"; "Contribute" button) and "Reviewers" (name + mono key; footnote "Released funds go to the project's shared account. No single key can move escrowed funds; the program checks reviewer approvals first.").

**Data:** `funding.get(proposalId)` mirrors `olb_funding` program state; `funding.approve(milestoneId)`, `funding.requestEvidence(milestoneId, note)`, `funding.contribute(amount, private)`.

**Rules:** release happens on-chain when approvals reach the threshold; UI never computes it locally. Currency label is "test tokens" on testnet.

---

## S5 · Publish bundle — `Publish.dc.html` (nav: Publish)

**Purpose:** choose what to reveal, check credit, publish the bundle publicly.

**Layout**
- Breadcrumb; H1 "Publish the preprint bundle"; lede.
- Panel "Provenance chain": `ProvenanceChain` Idea → Hypothesis → Funding → Experiments → Data → Manuscript (final node accent).
- Main column:
  - Panel "What to reveal": checkbox rows — reveal pre-registration (accent "Fingerprint matches the 14 Aug commitment"), reveal idea note (optional), make dataset public ("Decrypts 12 files; their addresses become public"), include negative result, include raw export (amber warning "Only 1 copy exists · pin it to the replica before publishing" + link).
  - Panel "Contributors and credit": `DataTable` Contributor / Role (CRediT) / Recorded work / Status badge (Accepted, Awaiting reply, Listed separately for the agent).
- Side panel "Publish": manuscript file box, Licence select (CC BY 4.0 default), "After publishing" checkboxes (open public review on Logos Messaging; export RO-Crate copy; invite reproduction), amber notice if any credit is unaccepted ("…their record stays open for 14 days"), **"Publish bundle"** (primary), footnote "Published files cannot be recalled. Copies spread as other nodes download them."

**Data:** `publish.prepare(ideaId)` returns chain, reveal options with verification results, credit table; `publish.execute(options)`.

**Rules:** block publish while any included file has fewer than 2 copies (show the amber warning; offer pin). Verifying a reveal = recomputing the commitment from salt + record and comparing with `olb_registry`.

---

## S6 · History (version control) — `History.dc.html` (nav: History)

**Purpose:** git-like history of an idea or the whole project. Spec of the model: `docs/05-version-control.md`.

**Layout**
- Header: H1 "History"; lede "Every change is a signed commit. Branch to try something without touching the main record, then merge it after review. Commits are anchored on LEZ in batches."; actions "Compare versions" (secondary), "New branch" (primary).
- Filters row: Branch select (mono: main, h2-computational, alt-buffer (merged)); Idea select (one idea / all ideas).
- Main column:
  - Panel "Commits on {branch}": list of `CommitRow`s newest first with a two-lane graph; "Show 40 older commits" link. Selected commit has violetTint background.
  - Panel "Changes in {hash}": parent + signing key meta; `DiffBlock` per changed field; Files block (+ added, − removed, ~ changed); buttons "Open Run 14", "Restore this version", "Branch from here"; footnote "Restoring creates a new commit. Nothing in the history is ever rewritten."
- Side column:
  - Panel (violet 2 px) "Open merge request": title, `source → target` mono, badges (+3 records accent, 1 record edited amber, No conflicts slate), reviewers with status, "main needs 1 approval before a merge", buttons "Review changes" (violetPrimary), "Comment".
  - Panel "Branches": name (mono), status (Protected / n ahead / Merged), owner meta.
  - Panel "Tags": mono tag + date; "Tag current version".

**Badges on commit rows:** "Waiting for next anchor" (amber), "Batch {n}" (accent), "Merge · reviewed by {n}" (slate), branch name (violet mono), "Negative result" (slate), "tag {name}" (accent mono).

**Data:** `vcs.log({ideaId?, branch, limit, before})`, `vcs.diff(commitId)`, `vcs.branches()`, `vcs.tags()`, `vcs.mergeRequests()`, `vcs.restore(commitId)`, `vcs.branch(name, fromCommit)`, `vcs.tag(name, commitId)`, `vcs.reviewMerge(mrId)`.

**Graph rendering:** compute lanes in core (`lane`, `edges` per row) so QML only draws; support at least 3 lanes, collapse further lanes into a "+n branches" marker.

---

## S7 · Project members — `Members.dc.html` (nav: Members)

**Purpose:** see who is in the project, what they can do, invites in flight, replica nodes, key status.

**Layout**
- Header: meta project name; H1 "Members"; lede "Everyone here holds the project key and can read its private records. What they can change depends on their role."; **"Invite member"** (primary) opens M1.
- Panel "People (n)": table Member (name + ORCID linked / not linked / external) / Role badge / Ideas ("3 of 5") / Signing key (mono short) / Status (dot + Online / Last seen) / "Manage" button (opens a role/access/remove sheet — to be designed; use a simple dialog).
- Two columns:
  - Panel "What each role can do": permissions matrix (Read private records; Write entries and open branches; Approve merges into main; Commit hypotheses on LEZ; Approve funding milestones; Publish and invite members) × Lead / Contributor / Reviewer. Footnote "Researchers never approve funding for their own work; that stays with reviewers."
  - Side: "Replica nodes" (name, "Always on · keeps a copy of every file · cannot read or sign", "Add a replica node"); "Pending invites" (sent-by-ID row with amber "Not yet received by their node" + "Resend"; redeemed-code row with violet "Waiting for your approval" + primary "Review"); "Project key" ("Version 3 · last rotated 2 Oct" + rotation explanation).

**Data:** `members.list()`, `members.roles()`, `invites.list()`, `replicas.list()`, `projectKey.status()`.

**Interactions:** "Invite member" opens M1 over a dimmed backdrop. "Review" on a redeemed code opens M1 at a review step (same as step 3 with the requester's fingerprint and an "Approve" primary). "Resend" re-publishes the same sealed envelope.

---

## S8 · New project — `NewProject.dc.html` (nav: "+ New project" active; no "This project" section)

**Purpose:** create a project with its stages, privacy, team, anchoring frequency and references.

**Layout**
- Breadcrumb Projects / New project; H1 "New project"; lede "…You can change everything except privacy later."
- Main column: six numbered panels (`1 · Basics` … `6 · References and links`), then footer actions right-aligned: "Cancel" (text link → back), "Save draft" (secondary), **"Create project"** (primary).
  1. **Basics**: Title (16 px input, required), One-line summary, Description (TextArea 5 rows), Start date, Expected duration (Under 6 months / 6 to 12 months / 1 to 2 years / Longer than 2 years), Default licence on publication (CC BY 4.0 / CC0 1.0 / CC BY-SA 4.0).
  2. **Field and team**: Research field select (Biochemistry and enzymology; Molecular and cell biology; Bioprocess engineering; Chemistry; Materials science; Computational biology; Other), Keywords (comma-separated), Team size pills (Just me / 2 to 5 people / 6 to 15 people / More than 15), Team members (input "Name, ORCID or Logos key" + role select + Add; list with Remove). Adding a member here queues an invite (M1 logic) sent on create.
  3. **Research stages**: six `CheckboxCard`s (number · name + one-line description). Warning (amber) "Choose at least one stage to create the project." when none.
  4. **Privacy**: three `RadioCard`s — Open / Private (default) / Invite only — each with description and an on-chain line (12 px ink3). Section note: cannot be changed later.
  5. **Version history and commits**: Anchoring frequency `RadioCard`s — Every change / Batched every few minutes (default, "Recommended") / Once a day / Only at key moments; "Approvals needed to merge into main" (None / 1 / 2); "Replica node for project files"; checkboxes "Let members open branches…" and "Tag a version automatically at pre-registration, milestone releases and publication".
  6. **References and links**: type select (Paper (DOI) / Dataset / Protocol / Preprint / Web page) + identifier input + Add; list with Remove; "Import BibTeX or RIS file", "Paste a list of DOIs"; optional "Existing grant or funder".
- Side column: "Preview on the board" — live mini idea card (chip "Stage 1 of {N} · {first stage}", privacy short label, title or "Untitled project" in ink3, `StageTracker` with only selected stages); definition list Privacy / Anchoring on LEZ / Team / Stages ("Idea → Hypothesis → …"). Panel "What happens when you create it" (4 steps; the last depends on privacy).

**Data:** `project.create(payload)` (schema in `docs/06-core-module-api.md`), `references.resolve(identifier)` (DOI/URL metadata lookup in core), `references.importBib(path)`.

**Validation:** title required (1–140 chars); at least one stage; privacy required; team members must parse as OLB ID or invite-code recipient. Errors appear under fields in amber with an icon-free message.

---

## M1 · Invite a member (modal) — `InviteDialog.dc.html` (+ `InviteCode`, `InviteStep2`, `InviteStep3`, `InviteStep4` show its states)

Opened from S7 "Invite member" (and S8 team list). Protocol: `docs/04-invite-flow.md`.

**Shell:** `Modal` 640 px. Header: meta project name, title "Invite a member", Close button, three-step bar (Identify · Role and access · Review and send). Footer: "Step n of 3" left; Back (steps 2–3), primary right ("Continue", "Continue", "Send invite"/"Activate code", "Done").

**Step 1 · Identify** — fieldset "How do you want to invite them?":
- RadioCard **"Paste their Open Labbook ID"** — "For someone who already uses Open Labbook. You check their key fingerprint, then send."
  - Input (mono) "Their Open Labbook ID", helper "They copy it from Open Labbook, Profile, Copy my ID. It carries their long-term sealing key, so it keeps working after they restart Basecamp."
  - On a valid ID: preview card — name (600) + "Name and ORCID as they set them · not verified by Open Labbook", presence ("Online now" accent dot / "Offline · will deliver when they return"), key fingerprint box (mono 14, 6 groups of 4 hex), checkbox "I compared this fingerprint with {name} in person or on a call".
  - Invalid ID: amber message under input "This is not an Open Labbook ID. Ask them to copy it again."
  - Continue enabled only when ID valid **and** fingerprint checkbox ticked.
- RadioCard **"Share a one-time invite code"** — "For someone new. They paste the code, their node replies with its key, you approve."
  - Code box: mono 22 "OLB-XXXX-XXXX-XXXX", "Copy code", "Make a new code"; Expires after (24 hours / 48 hours default / 7 days); Can be used (Once default / Up to 5 times); three numbered steps; note "The code holds no project data. It only lets their node find yours."

**Step 2 · Role and access** — Role RadioCards (Contributor / Reviewer / Lead with descriptions); "Which ideas can they see?" pills All ideas / Selected ideas; when Selected, checklist of ideas (44 px rows); note "Each idea's records are sealed separately, so access can be widened later."; optional note TextArea "Sent end-to-end encrypted with the invite".

**Step 3 · Review and send** — summary list (Member/Invite, Key fingerprint/Valid for, Role, Access); "What happens when you send" (4 lines, differ by method); amber testnet notice. **Copy change for implementation:** the design says the invite "travels as an end-to-end encrypted Logos Chat message" and the notice mentions Chat addresses. Implement delivery over a **Logos Delivery inbox topic** (see `docs/04-invite-flow.md`) and use this copy instead: line 2 "It travels end-to-end encrypted over Logos Messaging to their inbox."; notice "Testnet: if they are offline for long, the network may drop the message. Resend from Members, Pending invites — nothing else needs redoing."

**Step 4 · Sent** — title "Invite sent to {name}" / "Invite code is active"; subline; `ProgressList`: by ID — Project key sealed to their key (Done), Handed to Logos Delivery (Done), Received by their node (Waiting), Accepted, key installed (Not yet); by code — Invite code created, Code redeemed by their node, You approve their request, Project key sealed and delivered. Footer: "Invite another" (secondary), "Done" (primary, closes). Note "You can follow this in Members, Pending invites. A signed commit records the change in History once they accept."

**Modal behaviour:** Esc and Close dismiss (confirm only if step ≥ 2 has unsaved input); focus moves to the first field on open and back to "Invite member" on close; the backdrop does not close on click (prevents accidental loss).

---

## Cross-screen behaviours

- **Toasts**: brief confirmation after commits, sends and pins, bottom-right, 4 s, not for errors.
- **Errors**: inline amber panel with what failed and one action ("Retry"). Network-down errors never lose user input (drafts are local).
- **Times**: relative under 24 h ("6 min ago"), then "2 days ago" up to 7 days, then "9 Oct" (add year if not current). Tooltip shows full ISO time in the user's time zone.
- **Hashes and keys**: show short form; click to copy full; tooltip shows full value.
- **Numbers**: tokens and amounts in IBM Plex Mono with thousands separators.
