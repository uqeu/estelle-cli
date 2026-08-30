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
