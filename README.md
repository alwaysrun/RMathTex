# RMathTex

RMathTex is a Windows-only LaTeX equation editor written in Rust. You type LaTeX source in the editor at the bottom of the window, the preview pane above it re-renders the formula as live vector graphics, and one click copies the result into Word, PowerPoint or WPS as an editable equation — or exports it as SVG, PNG, DOCX or a Markdown snippet.

The application is a single native binary with no WebView, no bundled LaTeX distribution, no external processes and no network access. The user interface is in Chinese.

## Features

- **Live vector preview** — re-renders 80 ms after typing stops; output is native egui geometry, sharp at any DPI and on light or dark themes.
- **Office interop** — "Copy as Office" writes MathML, HTML and plain-text LaTeX to the clipboard in a single transaction, so Word, PowerPoint and WPS paste a native editable equation. "Import from Office" reverses it: copy an equation in Word, press the shortcut, and the LaTeX comes back normalized.
- **Four export formats** — self-contained SVG with glyph outlines, PNG at 144 / 300 / 600 DPI on a transparent or white background, a minimal DOCX containing a native OMML equation, and a single-formula Markdown fragment.
- **Markdown input and output** — accepts bare LaTeX as well as `$…$`, `$$…$$`, `\(…\)`, `\[…\]` and GitHub ```` ```math ```` fences. Output form can follow the input or be forced to inline, display or fenced.
- **Formula catalog** — 9 built-in categories (basic structures, Greek letters, operators and relations, sets and logic, arrows, calculus and large operators, functions and accents, matrices and cases, common university formulas). Clicking an entry inserts it at the cursor or replaces the selection.
- **Configurable favorites bar** — pick the formulas shown on the toolbar; the choice is stored in `RMathTex.json` next to the executable and survives restarts.
- **Explicit failure reporting** — the status bar shows the clipboard formats actually written, export paths and error codes. Unsupported constructs and conversion failures are reported instead of producing broken output.
- **Missing-glyph fallback** — the embedded math font has no CJK coverage; characters it lacks are drawn with a system font in the preview instead of failing the whole render. Preview refreshes are frozen while an IME composition is active.

## Requirements

| Item | Requirement |
| --- | --- |
| OS | Windows 10 or 11, x64 (any other target fails at compile time) |
| Toolchain | Rust 1.97 or newer, `stable-x86_64-pc-windows-msvc` |
| Runtime | Nothing else — no LaTeX, Python, browser engine or runtime libraries |

## Build

```powershell
cargo build --release --bin RMathTex
```

Or use the packaging script, which also copies the third-party license list into the output folder:

```powershell
pwsh ./scripts/build-crates.ps1
```

The binary is written to `target/release/RMathTex.exe`, or `output/RMathTex.exe` when the script is used.

## Usage

```powershell
cargo run --release --bin RMathTex
# or
./output/RMathTex.exe
```

The window is split vertically: the preview occupies the upper area, the LaTeX source editor sits below it at roughly one third of the height, and the divider between them can be dragged. Menus, the copy buttons, the Greek letter palette and the favorites bar are at the top; the status bar is at the bottom.

### Keyboard shortcuts

| Shortcut | Action |
| --- | --- |
| `Ctrl+Shift+C` | Copy as Office equation |
| `Ctrl+Shift+V` | Import equation copied in Word |
| `Ctrl+Shift+M` | Copy as Markdown plain text |
| `Ctrl+Enter` | Insert a blank line at the caret |

### Menus

| Menu | Contents |
| --- | --- |
| File (文件) | Export SVG / PNG / DOCX / Markdown, exit |
| Edit (编辑) | Copy to Office, import from Office, copy as Markdown, insert blank line |
| Formulas (公式) | The 9 built-in formula categories |
| Settings (设置) | Font size (10–32 pt), Display / Text style, theme, PNG background and DPI, Markdown output form, exit-confirmation policy |
| Favorites (常用公式) | Choose the formulas shown on the toolbar, restore defaults |

### Configuration

Settings and favorites are stored in `RMathTex.json` next to the executable. If the file cannot be read or written, the application falls back to defaults and reports the failure in the status bar instead of failing silently.

## Known limitations

- One formula at a time; this is not a full LaTeX document editor. TikZ, `\ce` / mhchem, PDF output and user-defined macros are not supported.
- SVG and PNG export use glyph outlines from the embedded font and do not support characters it lacks, such as Chinese text; Office clipboard and Markdown export are unaffected.
- Older WPS versions may negotiate the clipboard down to an image; use DOCX export in that case.

## Third-party licenses

The distributed binary ships with [THIRD_PARTY_LICENSES.txt](THIRD_PARTY_LICENSES.txt), which lists every third-party component, its license and the full license texts.

## License

This project is released under the GNU General Public License v3.0 (`GPL-3.0-only`). See [LICENSE](LICENSE) for the full terms.
