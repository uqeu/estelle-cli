# Actual Estelle TUI renderer gallery

These frames are evidence from the production `App` and `render_frame` implementation. Only the typed
server payloads are test fixtures. The layout, command renderer, production pane, picker, Orchestra, Todo,
composer, status line, dither ground, colours, truncation, and responsive geometry are production code.

Regenerate from `cli-rs/`:

```sh
ESTELLE_ACTUAL_GALLERY_DIR=docs/actual-gallery \
  cargo test -p estelle-tui --bin estelle actual_renderer_gallery_covers_the_product_surfaces
```

Open `index.html` to inspect the generated frames together. Each `.txt` file is the exact terminal-cell text;
each `.svg` preserves the corresponding Ratatui foreground, background, and bold styles.

This is the only renderer gallery. The former parallel visual fixture was deleted because it could
pass while the customer-facing TUI drifted. Layout and shipped behaviour are now judged from these
frames together; every frame enters through the production `render_frame` function.

## 41 frames, in two families

**18 come from live `App` state** through `render_frame`, exactly as before.

**23 come from `design_book`** — the twelve SPEC and four PROPOSED screens of
`CLI-DESIGN-BOOK.html`, plus seven SHIPPED renderer states the gallery had never captured (running
outside a repo, the gate refusal, the provider-key picker, the spend view, session tabs, a running
sweep). They are driven by fixtures because there is no live state that produces them on demand —
there is no `/doctor` failure to stage and no stale index to induce. **Their LAYOUT is not fixture:**
every row is built by `cols`, in `theme::Palette`'s colours, under the same box guard and the same
needle assertion as every live frame. The founder asked for the book to be rendered in Rust so the
port would be a copy rather than a translation; this is that.

⚠️ **THE DIRECTORY IS REGENERATED, NOT ADDED TO.** Until 2026-09-02 it also held twelve frames from
the 2026-08-06 import under an older numbering scheme — `03-orchestra-active` beside today's
`02-orchestra-active` — that no test had produced for weeks. Nothing was wrong with any of them
except that they were not true any more, and a directory listing cannot tell a current frame from a
stale one. **Delete the directory before regenerating**, or you are reading a mixture.

## The book

`../CLI-DESIGN-BOOK-RUST.html` is built from these frames by `../../scripts/design_book.py`. It reads
the colours back out of the SVGs rather than retyping them, so a colour that matches no
`theme::Palette` token is outlined on the page and counted in the script's report — which is how four
untokened-colour defects in the previous book were found.
