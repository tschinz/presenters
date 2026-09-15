<div align="center">

<img src="img/logo.png" alt="presenters logo" width="260">

# presenters

**A fast, native dual-screen PDF presenter for Typst & LaTeX slides — inspired by [pympress](https://github.com/Cimbali/pympress).**

Runs on macOS · Windows · Linux · Built with [egui](https://github.com/emilk/egui) + [PDFium](https://pdfium.googlesource.com/pdfium/)

</div>

---

## What it does

Open a PDF exported from **Typst** or **LaTeX/Beamer** and drive a talk with two windows:

- 🖥️ **Presenter window** — the current slide, a next-slide preview, your speaker notes, and a talk timer.
- 📽️ **Audience window** — just the slide, on black, fullscreen on the projector.

It stays instant even on large decks (300+ pages) thanks to lazy rendering, a texture cache, and next-slide prefetch.

## Features

- **Works with or without notes.** Auto-detects Beamer/Typst "notes on second screen" decks (a double-width page whose right half is the notes) and splits the slide from the notes. Plain decks just work too.
- **Two windows, second-screen aware.** Start the audience window with `F5`, quit it with `Esc` — the presenter window keeps running. If a **second screen** is attached, the audience window is placed on it and fullscreened automatically (presenter stays on the primary); on a single screen it opens windowed. Double-click the audience window to toggle fullscreen.
- **Adaptive, resizable layout.** With notes, cycle 4 arrangements of current / next / notes; without notes, toggle a horizontal or vertical current + next split. Drag any divider to resize; sizes are remembered.
- **Talk timer + clock.** The footer shows the slide number, the current wall-clock time (with seconds), and a talk stopwatch that runs while you present, pauses when you quit the presentation, and resets with `R` — large and centered.
- **Adjustable footer size.** Make the slide counter and timer as big as you want.
- **Recent files + resume.** Launch with no file to pick from a recent-files list; reopening a deck resumes at the page you left off.
- **Remembers everything.** Window position, panel sizes, footer font, chosen layout, and recent files persist between sessions.
- **Fast on big decks.** Lazy, cached, prefetched rendering — no waiting on 300-page PDFs.

Settings live in a single JSON file in a per-OS config directory named `presenter`:
`~/.config/presenter/state.json` on macOS & Linux (honoring `$XDG_CONFIG_HOME`), and
`%APPDATA%\presenter\state.json` on Windows.

## Prerequisites: the PDFium library

Rendering uses Google's PDFium via [`pdfium-render`](https://crates.io/crates/pdfium-render), which loads the PDFium dynamic library at runtime. Fetch the prebuilt library for your platform (the app looks next to the binary, in a `lib/` subfolder, then in `third_party/pdfium/lib/`):

```bash
just setup-pdfium
```

This downloads the matching build from [bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries) for your OS/arch into `third_party/pdfium/` (git-ignored).

## Build & run

```bash
just run                 # open the example deck (no notes)
just run-notes           # open the example deck (with speaker notes)
just run file=deck.pdf   # open your own PDF
just run-empty           # start with no file — shows the recent-files screen
```

Or without `just`:

```bash
cargo run --release -- examples/04-aprog-ptr-en.pdf
```

Launch with no argument and press **O** (or click **Open**) to pick a file.

## Keyboard shortcuts

| Key | Action |
| --- | --- |
| `→` / `Space` / `PageDown` / scroll down | Next slide |
| `←` / `PageUp` / scroll up | Previous slide |
| `Home` / `End` | First / last slide |
| `F5` | Start presentation (audience window) |
| `Esc` | Quit presentation |
| `B` | Blank the audience screen (black) |
| `W` | Close the file, back to the start screen |
| `R` | Reset the talk timer |
| `L` | Flip layout (H/V with no notes; 4 presets with notes) |
| `+` / `−` | Footer font larger / smaller |
| `O` | Open a PDF |
| `P` | Toggle laser pointer / drawing |
| `D` | Delete all drawings |
| Double-click (audience) | Toggle fullscreen |

**Laser pointer & drawing:** hold the mouse button over the **current slide** in the presenter
window and a semi-transparent dot appears at that spot on the audience screen, following your
cursor; release to hide. Press **`P`** (or the header button) to toggle **drawing** mode, where
holding and dragging draws freehand lines on the slide instead; **`D`** deletes all drawings.
The **Pointer size −/+** buttons in the header set both the dot size and the line thickness.
Drawings are kept per slide while you navigate, and cleared when you close the file (never
saved to disk).

You can also **drag & drop a PDF onto the window** to open it. The **Shortcuts** button in
the header shows the key list in-app.

## Packaging

Build a native, self-contained bundle (icon + PDFium included) with
[cargo-bundle](https://github.com/burtonageo/cargo-bundle):

```bash
cargo install cargo-bundle   # once (also part of `just install`)
just bundle                  # bundle for the current OS (release)
```

Per-format recipes (run each **on its target OS** — cargo-bundle does not cross-build):

| Recipe | Output | Platform |
| --- | --- | --- |
| `just bundle-mac` | `Rust Presenters.app` | macOS |
| `just bundle-deb` | `.deb` | Linux (Debian/Ubuntu) |
| `just bundle-appimage` | `.AppImage` | Linux |
| `just bundle-msi` | `.msi` | Windows |

Bundles land under `target/release/bundle/<format>/`. The recipes run `ensure-pdfium`
first, so the matching PDFium library is downloaded and shipped inside the bundle; at
runtime the app finds it there (e.g. macOS `Contents/Resources/`) with no external setup.

## Development

```bash
just            # list all recipes
just test       # run tests
just clippy     # lint
just build      # release build into bin/
```

Tests are headless and need the PDFium library present (run `just setup-pdfium` first).

## License

Licensed under the MIT license ([LICENSE](LICENSE)).

This project links against and bundles the PDFium library (BSD-3-Clause); see
[THIRD_PARTY.md](THIRD_PARTY.md).
