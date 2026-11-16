# Clip Sheet

A Rust + AppKit menu bar app for keeping a small table within one click.

## Install

```sh
git clone https://github.com/the-sides/clip-sheet.git
cd clip-sheet
./install.sh
```

On macOS 14 or newer with Rust installed, run `./install.sh`. It builds and
launches `~/Applications/Clip Sheet.app`. No Dock icon, server, or web view.
When using jOS, run this folder's installer without cloning again.

## Use

1. Copy a CSV or Markdown table (or cells from a spreadsheet).
2. Click the table icon in the menu bar, then **Paste**.
3. Left-click any cell to copy its exact value. Right-click to edit; Return or
   clicking away finishes editing. Changes are captured as you type.
4. **+ Row** and **+ Column** append empty cells. Scroll for larger tables.
5. **Clear** permanently removes the sheet and brings back the centered Paste
   button. Clearing does not erase the system clipboard.

The current sheet stays in memory while the app runs and reappears whenever you
open the popover. Quit/restart loses it. Nothing is saved or sent anywhere.

Supports quoted CSV (including multiline values), TSV, semicolon-separated
values, and Markdown tables with escaped pipes. Markdown formatting around an
entire cell (`code`, **bold**) is removed. CSV cell spaces are preserved.
Imports are limited to 10,000 cells. Hover a cell to see text that is clipped.

## Develop

`cargo run`, `cargo test`, `cargo clippy -- -D warnings`.

`cargo run -- --smoke-test` exercises native import/copy/edit/add/clear actions
using a private pasteboard so it does not touch your clipboard. It does not
verify visual layout, popover placement, or keyboard focus.
