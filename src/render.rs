//! LaTeX → egui shapes with a cache keyed by [`RenderKey`].

use eframe::egui;
use latex_rust::{latex_to_shapes, Color, Dim, EguiOptions, MathFont};

use crate::model::{AppError, RenderKey, Rendered};

/// Gap between stacked preview lines, in units of the preview font size.
const LINE_GAP_EM: f32 = 0.35;
/// Minimum top-to-top advance of a stacked preview line, in units of the font size.
const LINE_ADVANCE_EM: f32 = 1.2;

/// Owns the single embedded STIX Two Math face.
pub struct Renderer {
    font: MathFont,
}

impl Renderer {
    /// Loads the embedded font once; failure is the only startup error path (E501).
    pub fn new() -> Result<Self, AppError> {
        let font = MathFont::stix_two_math()
            .map_err(|error| AppError::caused("E501", "内嵌 STIX 字体加载失败", error))?;
        Ok(Self { font })
    }

    /// Borrows the font so `export` can rasterize with the same face.
    #[must_use]
    pub fn font(&self) -> &MathFont {
        &self.font
    }

    /// Parses, lays out and tessellates `src` at `origin`.
    ///
    /// The returned shapes carry absolute coordinates: callers must not
    /// translate them, they only re-render when `origin` moves.
    ///
    /// LaTeX treats a newline as whitespace, but this editor is line oriented:
    /// hard newlines outside braces and environments paint on their own row.
    /// If any row fails to parse the whole expression is rendered as one block
    /// so a valid formula never degrades into an error.
    pub fn render(
        &self,
        src: &str,
        key: RenderKey,
        origin: egui::Pos2,
    ) -> Result<Rendered, AppError> {
        let mut options = EguiOptions::new();
        options.font_size_pt = Dim::from_i64(key.font_size_pt);
        options.color = Color::rgb(key.color[0], key.color[1], key.color[2]);
        options.display = key.display;
        let pixels_per_point = f32::from_bits(key.ppp_bits);
        let lines = preview_lines(src);
        if lines.len() > 1 {
            if let Ok(rendered) =
                self.render_stacked(&lines, &options, key, origin, pixels_per_point)
            {
                return Ok(rendered);
            }
        }
        let (shapes, rect) = latex_to_shapes(src, &self.font, &options, origin, pixels_per_point)
            .map_err(map_error)?;
        Ok(Rendered { key, shapes, rect })
    }

    /// Renders one row per line and stacks them from `origin` downwards.
    fn render_stacked(
        &self,
        lines: &[&str],
        options: &EguiOptions,
        key: RenderKey,
        origin: egui::Pos2,
        pixels_per_point: f32,
    ) -> Result<Rendered, AppError> {
        let font_size = key.font_size_pt as f32;
        let gap = font_size * LINE_GAP_EM;
        let min_advance = font_size * LINE_ADVANCE_EM;
        let mut shapes = Vec::new();
        let mut bounds = egui::Rect::NOTHING;
        let mut top = origin.y;
        for line in lines {
            let line_origin = egui::pos2(origin.x, top);
            let (mut line_shapes, rect) =
                latex_to_shapes(line, &self.font, options, line_origin, pixels_per_point)
                    .map_err(map_error)?;
            shapes.append(&mut line_shapes);
            bounds = bounds.union(rect);
            top += (rect.height() + gap).max(min_advance);
        }
        Ok(Rendered {
            key,
            shapes,
            rect: bounds,
        })
    }
}

/// Splits a body at hard newlines: depth-0 braces and environments keep theirs.
///
/// Trimmed results are returned; blank lines are dropped so trailing newlines
/// and blank separators do not allocate empty rows.
fn preview_lines(src: &str) -> Vec<&str> {
    let bytes = src.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0usize;
    let mut braces = 0usize;
    let mut environments = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => {
                let name_start = index + 1;
                let mut name_end = name_start;
                while name_end < bytes.len() && bytes[name_end].is_ascii_alphabetic() {
                    name_end += 1;
                }
                if name_end == name_start {
                    // Escaped character such as `\\` or `\{`; both bytes are literal.
                    index = name_start + 1;
                } else {
                    match &src[name_start..name_end] {
                        "begin" => environments += 1,
                        "end" => environments = environments.saturating_sub(1),
                        _ => {}
                    }
                    index = name_end;
                }
                continue;
            }
            b'{' => braces += 1,
            b'}' => braces = braces.saturating_sub(1),
            b'\n' if braces == 0 && environments == 0 => {
                push_preview_line(&mut lines, &src[start..index]);
                start = index + 1;
            }
            _ => {}
        }
        index += 1;
    }
    push_preview_line(&mut lines, &src[start..]);
    lines
}

fn push_preview_line<'a>(lines: &mut Vec<&'a str>, line: &'a str) {
    let trimmed = line.trim();
    if !trimmed.is_empty() {
        lines.push(trimmed);
    }
}

fn map_error(error: latex_rust::Error) -> AppError {
    let code = if matches!(error, latex_rust::Error::Parse(_)) {
        "E201"
    } else {
        "E202"
    };
    let message = if code == "E201" {
        "预览解析失败"
    } else {
        "预览布局失败"
    };
    AppError::caused(code, message, error)
}

#[cfg(test)]
mod tests {
    use super::preview_lines;

    #[test]
    fn splits_hard_newlines_only_at_depth_zero() {
        let source =
            "a + b\n\\frac{x\n}{y}\n\\begin{pmatrix}\n1 & 2 \\\\\n3 & 4\n\\end{pmatrix}\nc";
        assert_eq!(
            preview_lines(source),
            vec![
                "a + b",
                "\\frac{x\n}{y}",
                "\\begin{pmatrix}\n1 & 2 \\\\\n3 & 4\n\\end{pmatrix}",
                "c",
            ]
        );
    }

    #[test]
    fn drops_blank_and_trailing_lines() {
        assert_eq!(preview_lines("x\n\n  y  \n"), vec!["x", "y"]);
    }
}
