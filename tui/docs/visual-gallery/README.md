# Estelle TUI visual gallery

These frames are deterministic design fixtures rendered by Ratatui's `TestBackend`.
Every frame is visibly labelled `DESIGN FIXTURE · NOT LIVE DATA`; the gallery is design
review evidence, not proof that a release-binary code path is wired to production.
They exercise terminal layout, truncation, color, empty states, overlays, and the
five-column orchestra shape without adding fixture data or a demo mode to the release
binary.

Regenerate the SVG, text, and contact-sheet artifacts from `cli-rs/`:

```sh
ESTELLE_VISUAL_GALLERY_DIR=docs/visual-gallery \
  cargo test -p estelle-tui --test visual_gallery \
  gallery_covers_the_requested_surfaces -- --exact --nocapture
```

Open `index.html` to review all frames together. The `.txt` versions preserve the exact
terminal cells for review and diffing; the `.svg` versions preserve foreground and
background colors for visual inspection.

The gallery covers:

- startup and actionable home state
- active orchestra with a five-column swarm grid
- completed orchestra with terminal outcomes
- production health and sourced issues
- a proposed repair diff
- the slash-command palette
- settings
- model routing and selection
- expanded Todo state with completed results retained
- collapsed Todo state with completed results retained
