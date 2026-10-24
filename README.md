# Open Labbook on Logos Basecamp — handoff package for Claude Code

This package hands the Open Labbook (OLB) Basecamp app from design to implementation. It contains everything Claude Code needs to build it: product context, the design system, screen-by-screen specs, protocols for invites and version control, a proposed core-module API, a phased build plan with acceptance criteria, sample data, and the original design source.

Prepared 25 October 2026 for Unblock Research. Target stack is pinned to **Logos Basecamp 0.3.0 / Testnet v0.3**.

## How to use it

1. Create an empty repository, for example `olb-basecamp`.
2. Copy this whole folder into the repository root.
3. Move `CLAUDE.md` to the repository root if it is not already there. Claude Code reads it automatically every session.
4. Open the repository in Claude Code and paste the kickoff prompt below.
5. Work phase by phase (`docs/07-build-plan.md`). Start each phase with that phase's prompt from the same file.

## What is in here

| Path | What it is | Read it when |
| --- | --- | --- |
| `CLAUDE.md` | Standing rules for Claude Code: stack, constraints, conventions, what not to do | Every session (automatic) |
| `docs/01-product-and-architecture.md` | What OLB is, the research lifecycle, the Logos building blocks and how OLB uses them | Before any code |
| `docs/02-design-system.md` | Colours, type, spacing, components, semantic colour meanings, QML mapping | Before any UI work |
| `docs/03-screens.md` | Every screen and the invite modal: layout, components, copy, data, states, interactions | When building a screen |
| `docs/04-invite-flow.md` | Identity model, Open Labbook ID format, invite-by-ID and invite-code protocols, key rotation | Phase 1 members work |
| `docs/05-version-control.md` | Commits, branches, merges, tags, diffs, restore, anchoring on LEZ | Phase 1 history work |
| `docs/06-core-module-api.md` | Proposed `olb_core` methods, payloads and events the QML UI calls | Before wiring UI to backend |
| `docs/07-build-plan.md` | Phases, tasks, acceptance criteria, test plan, and a prompt per phase | Start of every phase |
| `docs/08-open-questions.md` | Decisions not yet made and facts to verify in the Logos source | Whenever blocked |
| `design/tokens.json` | Design tokens in machine-readable form | UI work |
| `design/Theme.qml` | The tokens as a QML singleton, ready to drop in | UI work |
| `design/canvas/` | The original design files from the Claude design canvas (`.dc.html`) and their layout index | To check exact copy, spacing and structure |
| `fixtures/sample-project.json` | The demo project shown in every mockup, as data | Build the UI against fixtures before the backend exists |

## Kickoff prompt (paste into Claude Code)

```text
You are building Open Labbook, a Logos Basecamp app for collaborative research
(idea → hypothesis → funding → experiments → data/results → publishing).

Read CLAUDE.md, then docs/01 through docs/08 in order. Then:

1. Summarise back to me, in under 25 lines, what you will build in Phase 0 and
   which facts in docs/08-open-questions.md you must verify in the Logos source
   before writing code.
2. Inspect the logos-module-builder 0.3.0 templates (ui-qml-backend and rust)
   and the Storage, Delivery and Chat module API references linked in CLAUDE.md.
   Report the exact call signatures you will use and anything that contradicts
   docs/06-core-module-api.md.
3. Propose the repository layout for Phase 0 and wait for my go-ahead before
   scaffolding.

Do not invent Logos APIs. If something in these docs conflicts with the real
Logos source, the source wins: tell me and update the doc.
```

## Links

- Architecture and build plan doc (Claude Docs): https://claude.ai/artifact/XZd2wB69S8ksNMxqfbHvd2
- Design canvas (Claude Design): https://claude.ai/artifact/2cQinvK8ddDsTUndHUsgk2
- Logos docs: https://docs.logos.co

## Screenshots

The canvas can export each artboard as PNG (Share › Export in the canvas). Drop them into `design/screenshots/` with the artboard file names (`Main.png`, `Entry.png`, …). Claude Code can then open them as images while building. The `.dc.html` files in `design/canvas/` are the source of truth for copy and structure either way.
