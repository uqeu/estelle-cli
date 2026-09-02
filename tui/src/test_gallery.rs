use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use ratatui::buffer::Buffer;
use ratatui::style::Color;
use ratatui::style::Modifier;

const BACKGROUND: &str = "#101010";
const CELL_WIDTH: u16 = 9;
const CELL_HEIGHT: u16 = 18;

pub(crate) fn write_frame(output: &Path, name: &str, buffer: &Buffer) {
    fs::create_dir_all(output).expect("create actual gallery output");
    fs::write(output.join(format!("{name}.txt")), buffer_text(buffer))
        .expect("write actual text frame");
    fs::write(output.join(format!("{name}.svg")), buffer_svg(buffer))
        .expect("write actual SVG frame");
}

pub(crate) fn write_index(output: &Path, names: &[&str]) {
    let mut index = String::from(
        "<!doctype html><meta charset=\"utf-8\"><title>Estelle actual TUI gallery</title>\
         <style>body{background:#080808;color:#E9E6DC;font-family:system-ui;margin:32px}\
         h1{font-size:20px}p{color:#8f9396}h2{font-size:15px;color:#70C6CC;margin-top:36px}\
         img{display:block;width:min(100%,1500px);border:1px solid #292b2c;background:#101010}</style>\
         <h1>Estelle TUI · actual renderer gallery</h1>\
         <p>Every frame below was produced by the production render_frame function with test-only typed payloads.</p>",
    );
    for name in names {
        let _ = write!(
            index,
            "<h2>{name}</h2><img src=\"{name}.svg\" alt=\"{name}\">"
        );
    }
    fs::write(output.join("index.html"), index).expect("write actual gallery index");
}

pub(crate) fn buffer_text(buffer: &Buffer) -> String {
    let mut output = String::new();
    for y in 0..buffer.area.height {
        let mut row = String::new();
        for x in 0..buffer.area.width {
            row.push_str(buffer[(x, y)].symbol());
        }
        output.push_str(row.trim_end());
        output.push('\n');
    }
    format!("{}\n", output.trim_end_matches('\n'))
}

fn buffer_svg(buffer: &Buffer) -> String {
    let width = buffer.area.width * CELL_WIDTH + 32;
    let height = buffer.area.height * CELL_HEIGHT + 32;
    let canvas_background = if buffer.area.width == 0 || buffer.area.height == 0 {
        BACKGROUND.to_string()
    } else {
        let background = buffer[(0, 0)].bg;
        if background == Color::Reset {
            BACKGROUND.to_string()
        } else {
            color_hex(background)
        }
    };
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\">\n\
         <rect width=\"100%\" height=\"100%\" fill=\"{canvas_background}\"/>\n\
         <g font-family=\"SFMono-Regular, Menlo, Consolas, monospace\" font-size=\"14\" xml:space=\"preserve\">\n"
    );
    for y in 0..buffer.area.height {
        let baseline = 24 + y * CELL_HEIGHT;
        // Backgrounds may be coalesced because rectangles have explicit pixel widths.
        // Glyphs may not: browser font advance is not a terminal cell contract.
        let mut x = 0;
        while x < buffer.area.width {
            let cell = &buffer[(x, y)];
            let bg = cell.bg;
            let start = x;
            while x < buffer.area.width {
                let next = &buffer[(x, y)];
                if next.bg != bg {
                    break;
                }
                x += 1;
            }
            let pixel_x = 16 + start * CELL_WIDTH;
            if bg != Color::Reset && color_hex(bg) != canvas_background {
                let segment_width = (x - start) * CELL_WIDTH;
                let pixel_y = baseline.saturating_sub(14);
                let _ = writeln!(
                    svg,
                    "<rect x=\"{pixel_x}\" y=\"{pixel_y}\" width=\"{segment_width}\" height=\"{CELL_HEIGHT}\" fill=\"{}\"/>",
                    color_hex(bg),
                );
            }
        }
        for x in 0..buffer.area.width {
            let cell = &buffer[(x, y)];
            if !cell.symbol().trim().is_empty() {
                let pixel_x = 16 + x * CELL_WIDTH;
                let weight = if cell.modifier.contains(Modifier::BOLD) {
                    "700"
                } else {
                    "400"
                };
                let _ = writeln!(
                    svg,
                    "<text x=\"{pixel_x}\" y=\"{baseline}\" fill=\"{}\" font-weight=\"{weight}\">{}</text>",
                    color_hex(cell.fg),
                    xml_escape(cell.symbol()),
                );
            }
        }
    }
    svg.push_str("</g>\n</svg>\n");
    svg
}

fn color_hex(color: Color) -> String {
    match color {
        Color::Reset => "#E9E6DC".into(),
        Color::Black => BACKGROUND.into(),
        Color::Red | Color::LightRed => "#E25B55".into(),
        Color::Green | Color::LightGreen => "#67D391".into(),
        Color::Yellow | Color::LightYellow => "#E4BC5D".into(),
        Color::Blue | Color::LightBlue => "#65A8FF".into(),
        Color::Magenta | Color::LightMagenta => "#C28AC9".into(),
        Color::Cyan | Color::LightCyan => "#70C6CC".into(),
        Color::Gray | Color::White => "#E9E6DC".into(),
        Color::DarkGray => "#707478".into(),
        Color::Rgb(r, g, b) => format!("#{r:02X}{g:02X}{b:02X}"),
        Color::Indexed(index) => format!("rgb({index},{index},{index})"),
    }
}

fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    #[test]
    fn svg_places_each_box_drawing_glyph_on_its_terminal_cell() {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 6, 1));
        buffer.set_string(0, 0, "┌────┐", ratatui::style::Style::default());

        let svg = buffer_svg(&buffer);

        assert!(svg.contains("<text x=\"16\""), "{svg}");
        assert!(svg.contains("<text x=\"61\""), "{svg}");
        assert!(svg.contains(">┐</text>"), "{svg}");
    }
}
