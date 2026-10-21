# UI design system

How Inbetween's interface looks and how to keep it consistent. Read this before you touch
`crates/ui-egui`. The rules follow the Refactoring UI method: hierarchy from weight and colour
before size, every value from a fixed scale, more space between groups than within them, one
primary action per window.

## Source of truth: `crates/ui-egui/src/theme.rs`

Every colour, text size, gap, radius, control height and shadow is a field of `Theme`
(`Theme::dark()` is the shipped theme). Nothing else defines visual values.

- `theme::install(ctx, theme)` builds egui's global `Style` from the tokens (`Theme::style`) and
  stores the theme in the context. The shell calls it once, on the first frame.
- `theme::get(ctx)` returns the installed theme. Windows read tokens from it (`t.color.text_secondary`,
  `t.space.xl`, `t.radius.card`) and never write literal colours or sizes.
- To change the look, change a value in `Theme::dark()`. Plain egui widgets pick it up through the
  global style, and redesigned windows through `theme::get`. To add a theme, add another
  constructor and install it.
- New tokens go into the struct with a doc comment, and the tests in `theme.rs` keep the scales honest:
  spacing on the 4 px grid, and text colours at WCAG AA (4.5:1) on their backgrounds.

### The tokens (Theme::dark)

| Group | Tokens |
|---|---|
| Surfaces | backdrop (black, 55%), app `#15171C`, footer `#1A1D23`, surface `#1E2128`, field `#262A33`, field_hover `#2F3440`, field_active `#363C4A` |
| Lines | edge `#2C313B` (window against backdrop), line `#333844` (hairlines, inputs), dash `#6A7282` (empty drop targets) |
| Text | text `#E8EAEF`, text_secondary `#A3AAB8` (6.2:1 on field), text_tertiary `#8A92A2` (4.6:1 on field), text_disabled `#6A7282` |
| Accent (olive and sage) | accent `#4D5640` (primary fill, selected rows, text selection; white text 7.7:1), accent_hover `#5A6449`, accent_text `#B5D390` (links on dark, 9.7:1), focus `#A5C57F` (focus rings, drop targets, the current item, active edges) |
| Danger | danger_fill `#2E2127`, danger_soft `#47262C`, danger_line `#B4545A`, danger_text `#E0A39C`, danger_text_strong `#F6D4CF` |
| Type | Geist Regular and Medium only (Roboto Regular behind them for missing characters). title 18 medium, body 14, label 13 medium, small 12, mono 13 |
| Space | xs 4, sm 8, md 12, lg 16, xl 24, xxl 32 |
| Radius | small 4, control 6, card 8, window 12 |
| Controls | height 36 (dialogs), compact_height 28 (inside cards, dense tools) |
| Shadows | window (0, 12, blur 32, 50% black), popup (0, 4, blur 12, 38% black), control (0, 1, blur 2, 40% black; under primary buttons) |

## Components: `crates/ui-egui/src/kit.rs`

Build redesigned windows from these, not from raw egui calls with styling:

- `kit::dialog(ctx, id, title, subtitle, width, body, footer)`: a centred modal: title in
  medium, a one-line secondary subtitle, a body padded `xl`, and a darker footer band. Escape
  closes; a click on the backdrop does not.
- `kit::button(ui, kind, text, enabled, height)`: `Primary` (the one action, accent fill),
  `Quiet` (Cancel and other footer actions), `Secondary` (small actions in cards), `Danger`
  (actions inside an error).
- `kit::field_label(ui, label, aside)`: a medium secondary label with an optional tertiary aside
  ("Optional") at the right. Mark optional fields, not required ones.
- `kit::text_field`, `kit::text_line` (truncating), `kit::icon` and `kit::icon_badge` (stroke icons
  on a 24-unit grid), `kit::dashed_rect`.

## Patterns

- **Window anatomy:** title, a subtitle that says what the window is for, body, and a footer with
  a context note at the left and the actions at the right (Quiet then Primary).
- **Labels are words, not parameter names.** "Start image", not `image1 *`. Commands keep their
  ids and parameters; only the shell's wording changes.
- **Show the default.** An optional field's hint is the value the command will use, worked out
  from what has been picked ("sun to moon").
- **Check early, say what to do.** Read a picked file at once and show any error where the file is,
  in a sentence a person can act on, with the engine's message in the tooltip.
- **Disabled until valid.** The primary button stays disabled until the command can succeed.
- **Metadata is small and tertiary.** Sizes, counts and formats use `text.small` in `text_tertiary`.

## Workflow for redesigning a window

1. Mock it up first (an HTML artboard using these tokens), iterate with the owner, then build.
2. Keep the command: the window builds the same params the CLI and MCP send. Bespoke windows
   replace the generic form only for their command (`InbetweenApp::invoke` routes the id).
3. Put the state in a pure module with unit tests (`new_two.rs`) and the drawing in
   `app/<name>_ui.rs`; add a kittest file that opens it from the menu command, drives it and
   saves screenshots (`INBETWEEN_UI_SHOTS`). Look at the screenshots.

## Screens

| Screen | Status | Code | Mockup |
|---|---|---|---|
| Design tokens | built | `theme.rs`, `kit.rs` | https://claude.ai/artifact/5dr1MEfJBGfhfZce9xCFrp |
| New From Two Images | built | `new_two.rs`, `app/new_two_ui.rs`, `tests/new_two.rs` | https://claude.ai/artifact/5dr1MEfJBGfhfZce9xCFrp |
| Main window (menus, tool strip, sequence list, panels, transport) | todo (global theme applied; sequence rows are cramped at the default sidebar width) | | |
| Generic command form (`form_ui.rs`) | todo (global theme applied) | | |
| Export dialogs and progress | todo | | |
| Dot tools (select, crop, camera) | todo | | |
| Effects, adjust, captions, curves, track | todo | | |
| Face library, model, mix | todo | | |
