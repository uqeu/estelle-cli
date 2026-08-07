# Actual Estelle TUI renderer gallery

These frames are evidence from the production `App` and `render_frame` implementation. Only the typed
server payloads are test fixtures. The layout, command renderer, production pane, picker, Orchestra, Todo,
composer, status line, dither ground, colours, truncation, and responsive geometry are production code.

Regenerate from `cli-rs/`:

```sh
ESTELLE_ACTUAL_GALLERY_DIR=docs/actual-gallery \
  cargo test -p estelle-tui --bin estelle actual_renderer_gallery_covers_the_product_surfaces
```

Open `index.html` to inspect all fifteen frames together. Each `.txt` file is the exact terminal-cell text;
each `.svg` preserves the corresponding Ratatui foreground, background, and bold styles.

The separate `../visual-gallery/` directory is labelled `DESIGN FIXTURE - NOT LIVE DATA`. It is the binding
structural target, while this gallery is the evidence produced by the real renderer. A disagreement between
matching frames is a product finding: structure is judged against the visual fixture and shipped behaviour
is judged from this actual gallery. Regenerate and review both together.
