# Design source files

These are the artboards from the Claude design canvas "Open Labbook — Basecamp UI", exported as source on 25 October 2026. They are **reference, not code to ship**: the app is built in QML.

## How to read a `.dc.html` file

- Each file is one screen (a "Design Component"). Markup inside `<x-dc>` is plain HTML with inline styles: copy, spacing, colours and structure are exact.
- `{{name}}` placeholders are filled from the `renderVals()` function in the `<script type="text/x-dc">` block at the bottom. Read that function to see the data and the interactive state (filters, selected options, steps).
- `<sc-for list="{{items}}" as="item">` repeats its content per item; `<sc-if value="{{flag}}">` shows content conditionally; `<dc-import name="X">` embeds `X.dc.html`.
- `support.js` is the canvas runtime and is not included; the files will not render standalone in a browser. Use the canvas link or PNG exports to see them.
- `canvas.json` lists every artboard, its title and size.

## Files

| File | Screen | Spec |
| --- | --- | --- |
| `Main.dc.html` | S1 Project board | `docs/03-screens.md` S1 |
| `Entry.dc.html` | S2 Notebook entry | S2 |
| `Hypothesis.dc.html` | S3 Pre-register a hypothesis | S3 |
| `Funding.dc.html` | S4 Funding and milestones | S4 |
| `Publish.dc.html` | S5 Publish bundle | S5 |
| `History.dc.html` | S6 History (version control) | S6 |
| `Members.dc.html` | S7 Project members (opens M1) | S7 |
| `NewProject.dc.html` | S8 New project | S8 |
| `InviteDialog.dc.html` | M1 Invite modal, step 1 by Open Labbook ID | M1 |
| `InviteCode.dc.html` | M1 step 1 by one-time code | M1 |
| `InviteStep2.dc.html` | M1 step 2 Role and access | M1 |
| `InviteStep3.dc.html` | M1 step 3 Review and send | M1 (note copy change) |
| `InviteStep4.dc.html` | M1 step 4 Sent | M1 |

Canvas link: https://claude.ai/artifact/2cQinvK8ddDsTUndHUsgk2
