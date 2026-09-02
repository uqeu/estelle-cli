//! The nine box-drawing corners and junctions, named once, plus the source-level guard that keeps
//! them out of every string this crate ships.
//!
//! 🔴 **THERE ARE NO BOXES IN ESTELLE.** `main.rs` already carries a `BOX_CORNERS` check over the
//! gallery frames, and it is blind BY CONSTRUCTION to every widget that builds its own `Line`s
//! rather than rendering through `render_frame`. On 2026-08-31 that blind spot held **fifteen
//! live production sites** emitting a `\u{2514}` tool-receipt tree connector — in `diff_render`,
//! `exec_cell`, four `history_cell` modules, `multi_agents`, `resume_picker` and
//! `status_indicator_widget` — and not one frame test could see any of them. A guard that only
//! covers the path someone remembered to instrument is a guard on that path, not on the system.
//!
//! This module closes the hole from the other side: over the SOURCE, so a call site is caught
//! whether or not any frame test happens to render it.
//!
//! ⚠️ **EVERY GLYPH BELOW IS SPELLED AS A `\u{…}` ESCAPE, AND THAT IS LOAD-BEARING.** The guard
//! searches source text for the raw glyph bytes. Written the way a reader would prefer, this file
//! would find itself, and the only route back to green would be an exemption for it — and an
//! assertion with a carve-out is where the next box hides. The escapes are what let the guard run
//! with **no exemption list at all**, including for itself.
//!
//! `\u{2502}` (`│`) and `\u{2500}` (`─`) are deliberately absent: a sub-line marker and a
//! horizontal rule are not corners, and corners are what make a box. `│` at a two-space indent is
//! the design's replacement for the connector this module bans.

/// Every corner and tee that makes a box, in reading order.
///
/// **One owner for the set.** It was hand-copied in three places before this existed and the
/// copies had already drifted: `main.rs` listed all nine, `wrapping.rs` listed six.
pub(crate) const BOX_CORNERS: [char; 9] = [
    '\u{250C}', // down and right
    '\u{2510}', // down and left
    '\u{2514}', // up and right
    '\u{2518}', // up and left
    '\u{251C}', // vertical and right
    '\u{2524}', // vertical and left
    '\u{252C}', // down and horizontal
    '\u{2534}', // up and horizontal
    '\u{253C}', // vertical and horizontal
];

/// True when `token` is exactly one box corner and nothing else.
pub(crate) fn is_lone_box_corner(token: &str) -> bool {
    let mut chars = token.chars();
    match (chars.next(), chars.next()) {
        (Some(only), None) => BOX_CORNERS.contains(&only),
        _ => false,
    }
}

#[cfg(test)]
mod source_guard {
    use std::collections::BTreeSet;
    use std::path::Path;
    use std::path::PathBuf;

    use super::BOX_CORNERS;
    use crate::style::brand_palette_guard::strip_comments;

    /// The gate token, assembled rather than written, for the same reason the glyphs above are
    /// escaped: this file's own controls quote `cfg(test)` items as fixture text, and a scanner
    /// that reads its own fixtures as real gates would blank code it should be reading.
    const CFG_TEST: &str = concat!("#[cfg", "(test)]");

    /// Attribute lines tolerated between the gate and the item it gates. Rule 2: the bound is
    /// named, not a literal buried in the loop.
    const MAX_ATTRS_BEFORE_ITEM: usize = 4;

    /// A floor on the files the walker must find. A walk that silently returns nothing is the
    /// vacuity failure this whole guard exists to prevent, so it is asserted, not assumed.
    const MIN_FILES_SCANNED: usize = 200;

    fn src_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
    }

    // ---------------------------------------------------------------- literals

    /// If a literal starts at `index`, the index one past its end; otherwise `None`.
    ///
    /// Needed because `{`, `}` and `;` all appear inside this crate's string and char literals,
    /// and a brace matcher that counts those runs off the end of the item it is measuring.
    fn skip_literal(chars: &[char], index: usize) -> Option<usize> {
        let current = *chars.get(index)?;
        if current == 'r' || current == 'b' {
            let mut cursor = index;
            if chars.get(cursor) == Some(&'b') {
                cursor += 1;
            }
            if chars.get(cursor) == Some(&'r') {
                cursor += 1;
                let hash_start = cursor;
                while chars.get(cursor) == Some(&'#') {
                    cursor += 1;
                }
                if chars.get(cursor) == Some(&'"') {
                    return Some(raw_string_end(chars, cursor + 1, cursor - hash_start));
                }
            }
            if current == 'b' && chars.get(index + 1) == Some(&'"') {
                return Some(string_end(chars, index + 2));
            }
            return None;
        }
        if current == '"' {
            return Some(string_end(chars, index + 1));
        }
        if current == '\'' {
            return char_literal_end(chars, index);
        }
        None
    }

    fn string_end(chars: &[char], from: usize) -> usize {
        let mut index = from;
        while index < chars.len() {
            match chars[index] {
                '\\' => index += 2,
                '"' => return index + 1,
                _ => index += 1,
            }
        }
        chars.len()
    }

    fn raw_string_end(chars: &[char], from: usize, hashes: usize) -> usize {
        let mut index = from;
        while index < chars.len() {
            if chars[index] == '"' {
                let closed = (1..=hashes).all(|offset| chars.get(index + offset) == Some(&'#'));
                if closed {
                    return index + hashes + 1;
                }
            }
            index += 1;
        }
        chars.len()
    }

    /// `'x'` and `'\n'` are literals; `<'static>` is a LIFETIME and must not swallow the rest of
    /// the file looking for a closing quote. This crate is full of `Line<'static>`.
    fn char_literal_end(chars: &[char], index: usize) -> Option<usize> {
        if chars.get(index + 1) == Some(&'\\') {
            let mut cursor = index + 2;
            while cursor < chars.len() {
                if chars[cursor] == '\'' {
                    return Some(cursor + 1);
                }
                cursor += 1;
            }
            return Some(chars.len());
        }
        (chars.get(index + 2) == Some(&'\'')).then_some(index + 3)
    }

    // ------------------------------------------------------------ cfg(test)

    fn starts_with(chars: &[char], index: usize, needle: &str) -> bool {
        needle
            .chars()
            .enumerate()
            .all(|(offset, wanted)| chars.get(index + offset) == Some(&wanted))
    }

    /// One past the end of the item that begins at `from`: through the matching `}` for a braced
    /// item, or through the `;` for `mod tests;` and `use …;`.
    fn item_end(chars: &[char], from: usize) -> usize {
        let mut index = from;
        let mut depth = 0usize;
        while index < chars.len() {
            if let Some(next) = skip_literal(chars, index) {
                index = next;
                continue;
            }
            match chars[index] {
                '{' => depth += 1,
                '}' => {
                    if depth == 0 {
                        return index;
                    }
                    depth -= 1;
                    if depth == 0 {
                        return index + 1;
                    }
                }
                ';' if depth == 0 => return index + 1,
                _ => {}
            }
            index += 1;
        }
        chars.len()
    }

    /// Blank every `cfg(test)`-gated item, preserving newlines so a later hit still reports the
    /// line it is actually on. Input must already be comment-stripped.
    fn blank_cfg_test_items(code: &str) -> String {
        let chars: Vec<char> = code.chars().collect();
        let mut out = chars.clone();
        let mut index = 0usize;
        // Bounded by the source length; every branch advances `index` by at least one.
        while index < chars.len() {
            if !starts_with(&chars, index, CFG_TEST) {
                index += 1;
                continue;
            }
            let end = item_end(&chars, index + CFG_TEST.chars().count());
            for slot in out.iter_mut().take(end).skip(index) {
                if *slot != '\n' {
                    *slot = ' ';
                }
            }
            index = end.max(index + 1);
        }
        out.into_iter().collect()
    }

    /// Every line of `source` that puts a box corner into code that ships, as `(1-based line,
    /// the original line trimmed)`. Comments and `cfg(test)` items are removed first.
    fn corner_hits(source: &str) -> Vec<(usize, String)> {
        let scanned = blank_cfg_test_items(&strip_comments(source));
        let originals: Vec<&str> = source.lines().collect();
        scanned
            .lines()
            .enumerate()
            .filter(|(_, line)| line.chars().any(|c| BOX_CORNERS.contains(&c)))
            .map(|(index, _)| {
                let text = originals.get(index).copied().unwrap_or_default();
                (index + 1, text.trim().to_string())
            })
            .collect()
    }

    // ------------------------------------------------------------- the crate

    fn rs_files(root: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    files.push(path);
                }
            }
        }
        files.sort();
        files
    }

    /// `mod x;` / `pub mod x;` / `pub(crate) mod x;` → `x`. An inline `mod x {` is not a file.
    fn declared_module_name(line: &str) -> Option<String> {
        let rest = line
            .strip_prefix("pub(crate) ")
            .or_else(|| line.strip_prefix("pub "))
            .unwrap_or(line);
        let name = rest.strip_prefix("mod ")?.strip_suffix(';')?;
        name.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            .then(|| name.to_string())
    }

    /// The files `file` pulls in only under `cfg(test)`, read off its own declarations.
    fn declared_test_modules(file: &Path, source: &str) -> Vec<PathBuf> {
        let Some(parent) = file.parent() else {
            return Vec::new();
        };
        let stem = file
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        // Submodules of `lib.rs` / `main.rs` / `mod.rs` sit beside it; submodules of `foo.rs`
        // live in `foo/`.
        let module_dir = if matches!(stem.as_str(), "lib" | "main" | "mod") {
            parent.to_path_buf()
        } else {
            parent.join(&stem)
        };
        let lines: Vec<&str> = source.lines().collect();
        let mut found = Vec::new();
        for (index, line) in lines.iter().enumerate() {
            if line.trim() != CFG_TEST {
                continue;
            }
            let mut path_override: Option<String> = None;
            for offset in 1..=MAX_ATTRS_BEFORE_ITEM {
                let Some(next) = lines.get(index + offset).map(|line| line.trim()) else {
                    break;
                };
                if let Some(rest) = next.strip_prefix("#[path = \"") {
                    path_override = rest.strip_suffix("\"]").map(str::to_string);
                    continue;
                }
                if next.starts_with('#') {
                    continue;
                }
                let Some(name) = declared_module_name(next) else {
                    break;
                };
                found.push(match &path_override {
                    Some(relative) => parent.join(relative),
                    None => {
                        let flat = module_dir.join(format!("{name}.rs"));
                        if flat.exists() {
                            flat
                        } else {
                            module_dir.join(&name).join("mod.rs")
                        }
                    }
                });
                break;
            }
        }
        found
    }

    /// The files this crate compiles ONLY under `cfg(test)`.
    ///
    /// **Derived from the crate's own declarations, never listed.** A hardcoded set would have
    /// exactly the hole this guard exists to close: the next test module would arrive unguarded,
    /// or — worse — a production file would be quietly parked on the list. `foo.rs` owns the
    /// sibling `foo/` directory in the 2018 layout, so marking a root marks its subtree.
    fn test_only_files(files: &[PathBuf]) -> BTreeSet<PathBuf> {
        let mut marked = BTreeSet::new();
        let mut roots = Vec::new();
        for file in files {
            let Ok(source) = std::fs::read_to_string(file) else {
                continue;
            };
            roots.extend(declared_test_modules(file, &source));
        }
        for root in roots {
            let subtree = if root.file_name().is_some_and(|name| name == "mod.rs") {
                root.parent().map(Path::to_path_buf)
            } else {
                Some(root.with_extension(""))
            };
            marked.insert(root);
            let Some(subtree) = subtree else { continue };
            for file in files {
                if file.starts_with(&subtree) {
                    marked.insert(file.clone());
                }
            }
        }
        marked
    }

    fn relative(root: &Path, file: &Path) -> String {
        file.strip_prefix(root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/")
    }

    // ------------------------------------------------------------ the assertion

    #[test]
    fn nothing_this_crate_ships_puts_a_box_corner_in_a_string() {
        let root = src_root();
        let files = rs_files(&root);
        let test_only = test_only_files(&files);
        let shipped: Vec<&PathBuf> = files.iter().filter(|f| !test_only.contains(*f)).collect();

        // Two vacuity guards, because "no offences" is exactly the shape that passes forever on a
        // scanner that read nothing: the walk must find the crate, and the test-only derivation
        // must not have swallowed it.
        assert!(
            files.len() >= MIN_FILES_SCANNED,
            "the walker found {} files under {} — it is not seeing the crate",
            files.len(),
            root.display()
        );
        assert!(
            shipped.len() >= MIN_FILES_SCANNED,
            "only {} of {} files were scanned — the cfg(test) derivation is over-marking and this \
             assertion is measuring almost nothing",
            shipped.len(),
            files.len()
        );

        let mut offences = Vec::new();
        for file in shipped {
            let Ok(source) = std::fs::read_to_string(file) else {
                continue;
            };
            for (line, text) in corner_hits(&source) {
                offences.push(format!("  {}:{line}  {text}", relative(&root, file)));
            }
        }

        assert!(
            offences.is_empty(),
            "a box-drawing corner reached a string this crate ships:\n{}\n\n\
             THERE ARE NO BOXES IN ESTELLE, and that holds for a connector that only looks like \
             part of one. A tool-receipt sub-line is `\"  \\u{{2502}} \"` — `\u{2502}` at a \
             two-space indent, in `palette.dim` — never `\u{2514}`, never `\u{251C}`, never \
             `\u{23BF}`. A border, a rule or a table edge has no replacement: delete it and carry \
             the relationship with indentation and columns, the way `production_hud.rs` does. \
             If a site genuinely needs a second vocabulary, RAISE IT — do not add an exemption \
             here. An assertion with a carve-out is where the next box hides.",
            offences.join("\n")
        );
    }

    // ------------------------------------------------------------- the controls
    //
    // 🔴 A GUARD THAT HAS NEVER BEEN RED IS DECORATION. The assertion above is "nothing was
    // found", which passes forever on a detector that cannot fire. These drive the real scanner
    // over source that DOES contain the drift and assert it is seen — and over every shape that
    // must NOT fire, so the green above is a verdict rather than a silence.

    /// Fixtures are BUILT from [`BOX_CORNERS`], never written out: a literal glyph in this file
    /// would make the guard find itself and force the exemption it refuses to have.
    fn fixture(corner: char, template: &str) -> String {
        template.replace('@', &corner.to_string())
    }

    #[test]
    fn the_scanner_sees_every_one_of_the_nine_corners() {
        for corner in BOX_CORNERS {
            assert_eq!(
                corner_hits(&fixture(corner, "let prefix = \"  @ \";")).len(),
                1,
                "the scanner is blind to a corner: {corner:?}"
            );
            assert_eq!(
                corner_hits(&fixture(corner, "if line.contains('@') { }")).len(),
                1,
                "the scanner is blind to a corner in a char literal: {corner:?}"
            );
        }
    }

    #[test]
    fn the_scanner_reads_code_and_not_commentary() {
        let corner = BOX_CORNERS[2];
        // A doc comment describing the connector that was removed is documentation, not a site.
        assert!(corner_hits(&fixture(corner, "/// the old `@` connector is gone")).is_empty());
        assert!(corner_hits(&fixture(corner, "// @\n/* @ */")).is_empty());
        assert!(corner_hits(&fixture(corner, "/* start\n@\nend */")).is_empty());
        // …but a `//` inside a string does not start a comment, so the hit still lands.
        assert_eq!(
            corner_hits(&fixture(corner, "let u = \"https://x\"; let p = \"@\";")).len(),
            1
        );
        // Code after a closing block comment is still code.
        assert_eq!(
            corner_hits(&fixture(corner, "/* x */ let p = \"@\";")).len(),
            1
        );
        // The reported line number is the line the corner is on, not the first line of the file.
        assert_eq!(
            corner_hits(&fixture(corner, "fn a() {}\nfn b() {}\nlet p = \"@\";"))[0].0,
            3
        );
    }

    #[test]
    fn the_scanner_ignores_cfg_test_items_and_stops_at_their_end() {
        let corner = BOX_CORNERS[2];
        let gated = format!("{CFG_TEST}\nmod tests {{\n    let p = \"{corner}\";\n}}\n");
        assert!(
            corner_hits(&gated).is_empty(),
            "a corner inside a cfg(test) item is not shipped and must not be reported"
        );
        // The blanking stops at the closing brace: code after the test module is still scanned.
        let after = format!("{gated}let shipped = \"{corner}\";\n");
        assert_eq!(corner_hits(&after).len(), 1, "{after}");
        // …and code BEFORE it always was.
        let before = format!("let shipped = \"{corner}\";\n{gated}");
        assert_eq!(corner_hits(&before).len(), 1);
        // A gated `use` or `mod x;` ends at the semicolon, not at some later brace.
        let semi = format!("{CFG_TEST}\nmod tests;\nlet shipped = \"{corner}\";\n");
        assert_eq!(corner_hits(&semi).len(), 1);
        // `not(test)` is production code and is NOT a gate.
        let negated = format!("#[cfg(not(test))]\nfn f() {{ let p = \"{corner}\"; }}\n");
        assert_eq!(corner_hits(&negated).len(), 1);
    }

    #[test]
    fn braces_in_literals_do_not_move_the_end_of_a_cfg_test_item() {
        let corner = BOX_CORNERS[2];
        // Every shape that breaks a naive brace matcher: a brace in a string, in a char literal,
        // in a raw string, and a lifetime that is not a char literal at all.
        let gated = format!(
            "{CFG_TEST}\nmod tests {{\n    let a = \"}}\";\n    let b = '}}';\n    \
             let c = r#\"}}\"#;\n    fn f(x: Line<'static>) -> &'static str {{ \"{corner}\" }}\n}}\n"
        );
        let after = format!("{gated}let shipped = \"{corner}\";\n");
        assert_eq!(
            corner_hits(&after).len(),
            1,
            "the cfg(test) item ended in the wrong place:\n{after}"
        );
    }

    #[test]
    fn the_scanner_does_not_panic_on_this_crates_source() {
        // Emoji, box glyphs and multi-byte punctuation are everywhere in these files; a byte-wise
        // scanner slices one in half and panics.
        let corner = BOX_CORNERS[0];
        let hits = corner_hits(&fixture(
            corner,
            "// 🔴 ╌╌ ─── was here\nlet p = \"@\"; // ⚠️ ╌╌",
        ));
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn test_only_files_are_derived_and_do_not_swallow_production() {
        let root = src_root();
        let files = rs_files(&root);
        let test_only = test_only_files(&files);
        let names: BTreeSet<String> = test_only.iter().map(|f| relative(&root, f)).collect();

        // Derived through all three declaration shapes this crate uses.
        for expected in [
            "history_cell/tests.rs",         // `mod tests;` from a mod.rs
            "history_cell/plans_tests.rs",   // `#[path = "…"] mod tests;`
            "chatwidget/tests/exec_flow.rs", // a submodule of a test-only module root
        ] {
            assert!(
                names.contains(expected),
                "the derivation missed a test-only file: {expected}"
            );
        }
        // …and files that ship are NOT on it. Over-marking is the failure that would make the
        // assertion above green over an unread crate.
        for shipped in [
            "diff_render.rs",
            "wrapping.rs",
            "exec_cell/render.rs",
            "history_cell/patches.rs",
            "status_indicator_widget.rs",
        ] {
            assert!(
                !names.contains(shipped),
                "the derivation marked a file that ships as test-only: {shipped}"
            );
        }
    }
}
