# 02 — Design system

The look is a quiet "lab ledger": cool off-white ground, white panels, near-black ink, one green accent, IBM Plex Sans for text and IBM Plex Mono for hashes, keys, CIDs and numbers. Tokens are in `design/tokens.json` and `design/Theme.qml`.

## 1. Colour

### Neutrals

| Token | Hex | Use |
| --- | --- | --- |
| `ground` | `#F5F6F3` | App background; sunken info boxes inside panels |
| `surface` | `#FFFFFF` | Panels, cards, inputs, side nav |
| `surfaceSunken` | `#ECEEE9` | Stage columns, progress track, row dividers |
| `line` | `#DDE0DA` | Panel and card borders, table header rules |
| `lineSoft` | `#ECEEE9` | Row dividers inside panels |
| `inputBorder` | `#C9CDC6` | Inputs, secondary buttons |
| `dashed` | `#B9BEB6` | Dashed "+ Add" tiles, offline dot, chain arrows |
| `ink` | `#15181C` | Primary text |
| `ink2` | `#4C535C` | Secondary text, field labels |
| `ink3` | `#646B73` | Meta text, captions (passes 4.5:1 on white) |

### Semantic colours (each has a text colour and a tint background)

| Meaning | Text / stroke | Tint | Used for |
| --- | --- | --- | --- |
| **Accent / verified / done** | `accent #0B6E4F` (hover `#08523B`) | `accentTint #E3F1EB`; selected-card fill `accentTintSoft #F3F9F6` | Primary buttons, active nav, anchored, done stages, copies OK, accepted credit |
| **Current / committed / in progress** | `violet #4B3FB0` | `violetTint #ECEAFB` | Current stage, committed or pre-registered items, commit hashes, branch names, current step in steppers, merge requests |
| **Attention / waiting** | `amber #8A4B00` | `amberTint #FBEFD9` | Needs approval, only 1 copy, waiting for anchor, testnet caveats, diff removals |
| **Neutral / draft / info** | `slate #3D4A5C` | `slateTint #E6EAF0` | Draft, protocol, negative result, private-to-group, merges, protected branch |

Rules: never use a semantic colour decoratively; never rely on colour alone (each badge has words); diff additions use accent on accentTint, removals amber on amberTint (not red/green, for colour-blind users).

Modal backdrop: `rgba(21, 24, 28, 0.48)`. Modal shadow: `0 24px 64px rgba(21, 24, 28, 0.24)`.

Dark mode is **not designed**. Build tokens so a dark set can be added later; do not ship one now.

## 2. Typography

Fonts: **IBM Plex Sans** 400/500/600 and **IBM Plex Mono** 400/500 (SIL Open Font License). Bundle the `.ttf` files inside `olb_ui` and load them with `FontLoader` — the QML sandbox only allows URLs inside the module directory.

| Token | Size / weight | Use |
| --- | --- | --- |
| `pageTitle` | 28 / 600, letter-spacing −0.01em, line-height 1.2 | Page H1 |
| `modalTitle` | 20 / 600 | Modal title |
| `cardTitle` | 18 / 600, line-height 1.3 | Idea card titles, "Ideas" section |
| `metric` | 22 / 600 Mono | Big numbers (funding summary) |
| `sectionTitle` | 16 / 600 | Panel H2 |
| `lede` | 15 / 400, colour ink2 | Page intro paragraph |
| `body` | 14 / 400, line-height 1.45 | Default |
| `bodyStrong` | 14 / 500 | Names, list titles |
| `small` | 13 / 400–500 | Breadcrumbs, helper text in panels, pills |
| `meta` | 12 / 400–500, colour ink3 | Meta lines, badges, field labels (500, ink2) |
| `overline` | 11 / 600, uppercase, letter-spacing 0.06em, ink3 | Nav section labels, chain node labels |
| `stageLabel` | 11 / 400–600 | Stage tracker labels |
| `mono` | 12–13 Mono | Hashes, CIDs, keys, fingerprints (fingerprint 14, letter-spacing 0.04em), codes (22, 0.06em) |

## 3. Spacing, radius, size

- Spacing scale (px): 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 24, 28, 32, 48.
- Page padding: 28 top, 32 sides, 48 bottom. Gap between page sections 20–24. Panel padding 20 (forms 24). Gap inside panels 12–16.
- Radius: 6 (progress), 7 (logo), 8 (inputs, buttons, sunken boxes), 10 (inner cards, radio cards, status rows), 12 (panels), 14 (idea cards), 16 (modal), 999 (pills, badges).
- Min touch target 44 px (buttons, nav items, inputs, list rows with actions). Filter pills 40 px.
- Side nav width 232 px; main content flexes. Two-column screens: main column `flex 999 1 560`, side column `flex 1 1 320–340`; they stack when the window is narrow.
- Borders 1 px; selected/emphasised cards 2 px (reduce padding by 1 px to keep size).

## 4. Components

Build these as reusable QML components in `olb_ui/qml/components/`. Names are suggestions.

| Component | Spec |
| --- | --- |
| `AppShell` | Row: `SideNav` (232) + content `Flickable` with page padding. Wraps to column under ~900 px. |
| `SideNav` | White, right border `line`. Logo block (28 px green rounded square with three white bars + "Open Labbook" 15/600 + "on Logos Basecamp" meta). Section "This project" (Board, Notebook, Hypotheses, Funding, Publish, History, Members) and "Projects" (project list + "+ New project" in accent). Items 44 px, radius 8; active = accentTint bg, accent text, 600. Bottom: node status box (dot, "Node online", "Logos testnet v0.3", "Synced with N of M members"). |
| `Breadcrumb` | 13 px ink3, links accent, "/" separators. |
| `PageHeader` | Optional overline/meta line, H1, badges row, right-aligned action buttons; wraps. |
| `Badge` | 12/500, padding 2×8, pill; variants accent, violet, amber, slate. |
| `Button` | 44 px, radius 8, padding 0×18, 14 px. `primary` (accent fill, white 600), `secondary` (white, inputBorder, ink 500), `violetPrimary` (violet fill; only for commit-to-chain and merge-request actions), `warning` (white, amber border and text 600), `soft` (accentTint fill, accent border, accent 600 — "Accept"). Disabled: 40% opacity. |
| `IconButton` | Square 44 px, radius 8, white fill + inputBorder (sunken on hover), glyph 18 px in `ink2` (`ink` on hover, `ink3` disabled). Icon-only, so it always carries both `Accessible.name` and a tooltip shown on hover **and** on keyboard focus. Takes `pathData` + `viewBox` from the `Icons` singleton. |
| `ChipLink` | 32 px pill, ground bg + line border (or violetTint for hypothesis links). |
| `Panel` | White, 1 px line, radius 12, padding 20, H2 16/600. |
| `StatusCard` | Panel variant: meta label, 16/600 value, 12 px sub-line (sub-line may be amber with an inline action link). |
| `StageTracker` | Grid of N equal columns (N = stages used), gap 4. Each: 6 px bar radius 3 + 11 px label. Done = accent bar + accent label 500; current = violet bar + ink label 600 (`aria-current=step`); upcoming = `line #DDE0DA` bar + ink3 label 400. Published = all done. |
| `IdeaCard` | Radius 14, padding 20, gap 16: stage chip (violet; slate for stage 1 drafts; accent "Published · all 6 stages") + privacy meta; title 18/600 link; summary ink2; `StageTracker`; 2×2 facts grid (meta label + 500 value); commit strip (ground bg, radius 8, 44 px: violet mono "branch · hash", message ellipsised, meta count) linking to History; footer meta (lead · people / updated). |
| `FilterPills` | Pills 40 px; selected = ink fill, white text; unselected = white, inputBorder. Count inside at 80% opacity. `aria-pressed`. |
| `RadioCard` / `CheckboxCard` | Label wrapping input (18 px, accent) + 600 title + 12–13 px description; border 2 px; selected = accent border + accentTintSoft fill; unselected = line border, white. |
| `TextField`, `TextArea`, `Select` | 44 px (TextArea padded 10×12), inputBorder 1 px, radius 8, label above 12/500 ink2, helper text 12 ink3 below. |
| `DataTable` | Header 12 px ink3 500 with `line` rule; rows 10–12 px padding with `lineSoft` rules; horizontal scroll wrapper when narrow. |
| `FileRow` | Name 500 + mono meta (size · CID shortened `logos:zb2rh…7hQe`) + copies badge (accent "4 copies" / amber "1 copy · only on this laptop") + optional "Pin to replica" warning button. |
| `SuggestionCard` | Agent edit: 12 px label, mono diff lines (− amber, + accent), optional amber "Inferred · confirm" badge, source note, Accept (soft) / Reject (secondary). |
| `Stepper` | Horizontal steps; done = accent filled circle with number; current = violet 2 px border card; upcoming = sunken circle. Modal variant: 4 px bars + 12 px labels. |
| `CommitRow` | 48 px graph gutter + content. Gutter: main lane line at x=12 (accent 2 px), branch lane at x=36 (violet 2 px), dots 12 px (white fill, 2 px border; merge dot 16 px solid accent), horizontal 2 px connector at fork and merge rows. Content: violet mono short hash, 500 message, one badge (anchor batch / waiting / tag / branch / negative result), meta line (author · signed · time · batch). Selected row: violetTint bg, radius 8. |
| `DiffBlock` | Bordered box, radius 8; header row (ground bg, 13/500 record + field); mono 12 lines, − on amberTint/amber, + on accentTint/accent. |
| `ProvenanceChain` | Wrapping row of node cards (overline stage, 500 title, 12 px status in semantic colour) separated by "→" in `dashed`; final node highlighted accent 2 px + accentTint. |
| `Modal` | Width 640 (max 100%), radius 16, shadow, max height window − 48 px. Header (meta project name, 20/600 title, Close button, stepper) / scrollable body / footer (step meta left; Back, secondary actions, primary right). Backdrop dims the page; Esc closes; focus trapped; returns focus to the opener. |
| `ProgressList` | Bordered list; each row: 12 px ring dot (done = accent filled; waiting = violet ring; later = inputBorder ring), label, right state word (Done / Waiting / Not yet). |

## 5. Iconography

The mockups use almost no icons; status is carried by dots, badges and words.
Icons are 16–20 px in `ink2`, and never appear without either a text label or
both an accessible name and a tooltip.

Icons live as **SVG path data in the `Icons` QML singleton**, not as `.svg`
files, and are drawn by `QtQuick.Shapes`. Three reasons: one glyph recolours
itself for light chrome, dark chrome and the disabled state instead of needing
three recoloured assets; a `Shape` stays crisp at any scale and DPI; and
nothing has to be added to the module package, which matters because QML UI
modules cannot read files outside their own directory (CLAUDE.md, platform
constraint 1). Each entry stores the path plus the viewBox edge it was authored
against, so glyphs drawn at different sizes mix freely.

The app icon is the exception — Basecamp reads it from `metadata.json` as a
PNG. It is generated by `design/icon/make-icon.py` from the artwork in the same
directory, so it can be regenerated rather than only edited.

## 6. Accessibility checklist

- Keyboard: tab order follows reading order; all actions reachable; Esc closes modals; focus ring visible (2 px accent outline, 2 px offset).
- Screen readers: set `Accessible.name`/`role` on custom controls; stage trackers expose the current step; status dots are decorative and duplicated in text.
- Contrast: all text tokens pass 4.5:1 on their intended backgrounds (checked for ink3 on white and on ground).
- Never truncate a hash or key without offering copy-full.
