//! LaTeX → egui shapes with a cache keyed by [`RenderKey`].
//!
//! The embedded STIX face has no CJK coverage. Characters it lacks are laid out
//! as same-width placeholders, then reported through [`Rendered::labels`] so the
//! app can paint them with a system font; a Chinese preview therefore neither
//! fails nor drops characters.

use eframe::egui;
use latex_rust::{
    latex_to_shapes, layout_with_em_size_pt, parse, shapes as emit_shapes, BoxContent, Color, Dim,
    EguiOptions, MathBox, MathFont, MathStyle,
};

use crate::model::{AppError, PreviewLabel, RenderKey, Rendered};

/// Gap between stacked preview lines, in units of the preview font size.
const LINE_GAP_EM: f32 = 0.35;
/// Minimum top-to-top advance of a stacked preview line, in units of the font size.
const LINE_ADVANCE_EM: f32 = 1.2;

/// Placeholder characters tried in order when the math face lacks a glyph.
///
/// All candidates exist in STIX Two Math and advance close to one em, so a
/// full-width character keeps its spacing. A candidate present in the source is
/// skipped, otherwise an original literal could not be told apart from one.
const PLACEHOLDER_CANDIDATES: [char; 3] = ['\u{25A1}', '\u{25A0}', '\u{25FB}'];

/// Centre of a full-width ideograph above the baseline, as a fraction of the em.
///
/// CJK faces place the ideographic em square roughly between `-0.12 em` and
/// `0.88 em`; centring the fallback ink there aligns it with the math baseline.
const CJK_INK_CENTER_EM: f32 = 0.38;

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
        self.render_line(src, &options, key, origin, pixels_per_point)
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
        let mut labels = Vec::new();
        let mut bounds = egui::Rect::NOTHING;
        let mut top = origin.y;
        for line in lines {
            let line_origin = egui::pos2(origin.x, top);
            let mut rendered =
                self.render_line(line, options, key, line_origin, pixels_per_point)?;
            shapes.append(&mut rendered.shapes);
            labels.append(&mut rendered.labels);
            bounds = bounds.union(rendered.rect);
            top += (rendered.rect.height() + gap).max(min_advance);
        }
        Ok(Rendered {
            key,
            shapes,
            labels,
            rect: bounds,
        })
    }

    /// Renders a single row, substituting glyphs the math face lacks.
    fn render_line(
        &self,
        src: &str,
        options: &EguiOptions,
        key: RenderKey,
        origin: egui::Pos2,
        pixels_per_point: f32,
    ) -> Result<Rendered, AppError> {
        match prepare_fallback(src, &self.font) {
            Some(prepared) => {
                self.render_with_fallback(&prepared, options, key, origin, pixels_per_point)
            }
            None => {
                let (shapes, rect) =
                    latex_to_shapes(src, &self.font, options, origin, pixels_per_point)
                        .map_err(map_error)?;
                Ok(Rendered {
                    key,
                    shapes,
                    labels: Vec::new(),
                    rect,
                })
            }
        }
    }

    /// Lays out the placeholder-rewritten line, then swaps every placeholder for
    /// a kern plus a [`PreviewLabel`] before tessellation.
    fn render_with_fallback(
        &self,
        prepared: &Prepared,
        options: &EguiOptions,
        key: RenderKey,
        origin: egui::Pos2,
        pixels_per_point: f32,
    ) -> Result<Rendered, AppError> {
        let ast = parse(&prepared.latex).map_err(|error| map_error(error.into()))?;
        let style = if options.display {
            MathStyle::Display
        } else {
            MathStyle::Text
        };
        let mut tree = layout_with_em_size_pt(&ast, &self.font, style, &options.font_size_pt)
            .map_err(map_error)?;
        let em_px = dim_to_f32(&options.font_size_pt) * pixels_per_point;
        let root_baseline = origin.y + dim_to_f32(&tree.height) * em_px;
        let mut glyphs = FallbackGlyphs::new(prepared);
        glyphs
            .collect(&mut tree, origin.x, root_baseline, em_px)
            .map_err(|()| fallback_mismatch())?;
        if glyphs.has_unmatched_substitutes() {
            return Err(fallback_mismatch());
        }
        let labels = glyphs.into_labels();
        let (shapes, rect) =
            emit_shapes(&tree, &self.font, options, origin, pixels_per_point).map_err(map_error)?;
        Ok(Rendered {
            key,
            shapes,
            labels,
            rect,
        })
    }
}

/// A line rewritten for the fallback path.
struct Prepared {
    /// Source with every unsupported character replaced by `placeholder`.
    latex: String,
    /// Character that stands in for the unsupported ones during layout.
    placeholder: char,
    /// Replaced characters in source order.
    substitutes: Vec<char>,
}

/// Rewrites characters the math face lacks, or returns `None` when it has them all.
fn prepare_fallback(src: &str, font: &MathFont) -> Option<Prepared> {
    let slots = fallback_slots(src, font);
    if !slots.iter().any(|(_, missing)| *missing) {
        return None;
    }
    let placeholder = choose_placeholder(src, font)?;
    let latex = slots
        .iter()
        .map(|(ch, missing)| if *missing { placeholder } else { *ch })
        .collect();
    let substitutes = slots
        .iter()
        .filter(|(_, missing)| *missing)
        .map(|(ch, _)| *ch)
        .collect();
    Some(Prepared {
        latex,
        placeholder,
        substitutes,
    })
}

/// Splits `src` into characters and whether the math face lacks their glyph.
///
/// Mirrors the tokenizer for the one case that drops characters instead of
/// drawing them: `%` comments (to the end of the line) and `\x` escapes.
fn fallback_slots(src: &str, font: &MathFont) -> Vec<(char, bool)> {
    let mut slots = Vec::with_capacity(src.len());
    let mut chars = src.chars();
    let mut comment = false;
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            slots.push((ch, false));
            if let Some(escaped) = chars.next() {
                slots.push((escaped, false));
            }
            continue;
        }
        if ch == '\n' {
            comment = false;
        } else if ch == '%' && !comment {
            comment = true;
        }
        slots.push((ch, !comment && needs_fallback(ch, font)));
    }
    slots
}

/// Whether the math face lacks a drawable glyph for `ch`.
///
/// Whitespace is never drawn as a glyph and ASCII is covered by the embedded
/// face, so both are excluded from the expensive coverage lookup.
fn needs_fallback(ch: char, font: &MathFont) -> bool {
    !ch.is_whitespace() && !ch.is_ascii() && font.glyph(ch).is_err()
}

/// Picks a placeholder absent from the source and present in the math face.
fn choose_placeholder(src: &str, font: &MathFont) -> Option<char> {
    PLACEHOLDER_CANDIDATES
        .into_iter()
        .find(|candidate| !src.contains(*candidate) && font.glyph(*candidate).is_ok())
}

/// Turns placeholder glyph boxes into [`PreviewLabel`]s during a tree walk.
struct FallbackGlyphs {
    placeholder: char,
    substitutes: std::vec::IntoIter<char>,
    labels: Vec<PreviewLabel>,
}

impl FallbackGlyphs {
    fn new(prepared: &Prepared) -> Self {
        Self {
            placeholder: prepared.placeholder,
            substitutes: prepared.substitutes.clone().into_iter(),
            labels: Vec::new(),
        }
    }

    /// Walks the box tree in emission order, recording where each fallback glyph
    /// belongs and replacing its ink with a kern of the same width.
    ///
    /// Coordinates follow the egui emitter: `x` is the left edge and `baseline`
    /// the parent baseline, both in shape units.
    fn collect(&mut self, bx: &mut MathBox, x: f32, baseline: f32, em_px: f32) -> Result<(), ()> {
        let baseline = baseline - dim_to_f32(&bx.shift) * em_px;
        let placeholder_width = match &bx.content {
            BoxContent::Glyph { ch, .. } if *ch == self.placeholder => Some(bx.width.clone()),
            _ => None,
        };
        if let Some(width) = placeholder_width {
            let Some(text) = self.substitutes.next() else {
                return Err(());
            };
            let size = dim_to_f32(&width) * em_px;
            self.labels.push(PreviewLabel {
                center: egui::pos2(x + size / 2.0, baseline - CJK_INK_CENTER_EM * size),
                size,
                text,
            });
            bx.content = BoxContent::Kern(width);
            return Ok(());
        }
        match &mut bx.content {
            BoxContent::HList(kids) => {
                let mut cursor = x;
                for kid in kids {
                    self.collect(kid, cursor, baseline, em_px)?;
                    cursor += dim_to_f32(&kid.width) * em_px;
                }
                Ok(())
            }
            BoxContent::VList(kids) => {
                let mut kids = kids.iter_mut();
                let Some(first) = kids.next() else {
                    return Ok(());
                };
                self.collect(first, x, baseline, em_px)?;
                let mut below = dim_to_f32(&first.depth) * em_px;
                for kid in kids {
                    let child_baseline = baseline + below + dim_to_f32(&kid.height) * em_px;
                    self.collect(kid, x, child_baseline, em_px)?;
                    below += (dim_to_f32(&kid.height) + dim_to_f32(&kid.depth)) * em_px;
                }
                Ok(())
            }
            BoxContent::Overlap(kids) => {
                for kid in kids {
                    self.collect(kid, x, baseline, em_px)?;
                }
                Ok(())
            }
            BoxContent::Color(_, inner) | BoxContent::BackColor(_, inner) => {
                self.collect(inner, x, baseline, em_px)
            }
            BoxContent::Frame { inner, .. } => self.collect(inner, x, baseline, em_px),
            BoxContent::Empty
            | BoxContent::Kern(_)
            | BoxContent::Rule
            | BoxContent::Line { .. }
            | BoxContent::Glyph { .. } => Ok(()),
        }
    }

    /// Whether substitutes remain that the layout never placed.
    fn has_unmatched_substitutes(&self) -> bool {
        !self.substitutes.as_slice().is_empty()
    }

    fn into_labels(self) -> Vec<PreviewLabel> {
        self.labels
    }
}

/// The layout engine can duplicate a glyph (delimiter variants) or drop it
/// (`\phantom`); labels can then no longer be mapped, so fail loudly instead of
/// painting characters at the wrong place.
fn fallback_mismatch() -> AppError {
    map_error(latex_rust::Error::Unsupported {
        what: "fallback glyphs do not align with the source".to_owned(),
    })
}

/// Converts a layout dimension to the `f32` units used by the egui emitter.
fn dim_to_f32(value: &Dim) -> f32 {
    f32::from_bits(value.to_ieee32_bits())
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
    use eframe::egui;

    use super::{preview_lines, Renderer};
    use crate::model::RenderKey;

    fn preview(src: &str) -> crate::model::Rendered {
        let renderer = Renderer::new().expect("embedded font");
        let key = RenderKey::new(src, 20, false, 1.0, [0, 0, 0]);
        renderer
            .render(src, key, egui::pos2(0.0, 0.0))
            .expect("preview renders")
    }

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

    #[test]
    fn draws_chinese_with_fallback_labels() {
        let rendered = preview(r"\text{中文}");
        let text: String = rendered.labels.iter().map(|label| label.text).collect();
        assert_eq!(text, "中文");
        assert!(rendered.labels[1].center.x > rendered.labels[0].center.x);
        assert!(rendered.rect.width() > 0.0);
    }

    #[test]
    fn keeps_literal_placeholder_characters() {
        let rendered = preview("\u{25A1}中");
        assert_eq!(rendered.labels.len(), 1);
        assert_eq!(rendered.labels[0].text, '中');
    }

    #[test]
    fn ignores_chinese_inside_comments() {
        let rendered = preview("x % 中文");
        assert!(rendered.labels.is_empty());
        assert!(!rendered.shapes.is_empty());
    }

    #[test]
    fn maps_fraction_arguments_in_source_order() {
        let rendered = preview(r"\frac{中}{文}");
        let text: String = rendered.labels.iter().map(|label| label.text).collect();
        assert_eq!(text, "中文");
    }
}
