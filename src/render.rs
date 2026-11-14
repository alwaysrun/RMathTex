//! LaTeX → egui shapes with a cache keyed by [`RenderKey`].
//!
//! The embedded STIX face has no CJK coverage. Characters it lacks are laid out
//! as same-width placeholders, then reported through [`Rendered::labels`] so the
//! app can paint them with a system font; a Chinese preview therefore neither
//! fails nor drops characters.

use std::cmp::Ordering;

use eframe::egui;
use latex_rust::{
    layout_with_em_size_pt, parse, shapes as emit_shapes, ttf_parser, BoxContent, Color, Dim,
    EguiOptions, MathBox, MathFont, MathStyle,
};

use crate::model::{source_rows, AppError, ExportStyle, PreviewLabel, RenderKey, Rendered};

/// Gap between stacked preview lines, in units of the preview font size.
const LINE_GAP_EM: f32 = 0.35;
/// Minimum top-to-top advance of a stacked preview line, in units of the font size.
const LINE_ADVANCE_EM: f32 = 1.2;
/// [`LINE_GAP_EM`] as an exact ratio for the export layout engine.
const EXPORT_LINE_GAP: (i64, i64) = (7, 20);
/// [`LINE_ADVANCE_EM`] as an exact ratio for the export layout engine.
const EXPORT_LINE_ADVANCE: (i64, i64) = (6, 5);

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

/// Upright bar glyphs: `vmatrix` delimiters and `\left|…\right|`.
///
/// STIX Two Math carries no vertical MATH variants for either, so the layout
/// engine can only draw the base glyph and every bar stays about one em tall.
const BAR_GLYPHS: [char; 2] = ['|', '‖'];

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

    /// Lays out `src` for the file exporters with the preview's corrections.
    ///
    /// The exporters serialize a laid-out tree; sharing [`Self::layout_row`]
    /// keeps their delimiters consistent with the preview. Hard newlines
    /// outside braces and environments stack rows with the preview's spacing,
    /// and blank rows become the same empty advance the preview paints.
    pub fn layout_export(&self, src: &str, style: &ExportStyle) -> Result<MathBox, AppError> {
        let em = Dim::from_i64(style.font_size_pt);
        let rows = source_rows(src);
        let visible = rows.iter().filter(|row| !row.is_empty()).count();
        if visible <= 1 {
            return self
                .layout_row(src, &em, style.display)
                .map_err(|error| AppError::caused("E601", "公式渲染失败", error));
        }
        self.layout_rows(&rows, &em, style.display)
            .map_err(|error| AppError::caused("E601", "公式渲染失败", error))
    }

    /// Stacks one row per source line, inserting the preview's vertical gaps.
    fn layout_rows(
        &self,
        rows: &[&str],
        em: &Dim,
        display: bool,
    ) -> Result<MathBox, latex_rust::Error> {
        let mut children = Vec::new();
        let mut previous: Option<Dim> = None;
        for row in rows {
            if let Some(span) = previous.take() {
                children.push(row_spacer(&span));
            }
            if row.is_empty() {
                previous = Some(Dim::zero());
            } else {
                let row_box = self.layout_row(row, em, display)?;
                previous = Some(&row_box.height + &row_box.depth);
                children.push(row_box);
            }
        }
        Ok(MathBox::vpack(children))
    }

    /// Parses, lays out and corrects one row of math.
    fn layout_row(&self, src: &str, em: &Dim, display: bool) -> Result<MathBox, latex_rust::Error> {
        let ast = parse(src)?;
        let style = if display {
            MathStyle::Display
        } else {
            MathStyle::Text
        };
        let mut tree = layout_with_em_size_pt(&ast, &self.font, style, em)?;
        stretch_bar_delimiters(&mut tree, &self.font);
        Ok(tree)
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
        let rows = source_rows(src);
        let visible = rows.iter().filter(|row| !row.is_empty()).count();
        if visible > 1 {
            if let Ok(rendered) =
                self.render_stacked(&rows, &options, key, origin, pixels_per_point)
            {
                return Ok(rendered);
            }
        }
        self.render_line(src, &options, key, origin, pixels_per_point)
    }

    /// Renders one row per line and stacks them from `origin` downwards.
    ///
    /// Blank rows emit no shapes but still advance the cursor, so a blank line
    /// in the source becomes visible vertical spacing.
    fn render_stacked(
        &self,
        rows: &[&str],
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
        for row in rows {
            if row.is_empty() {
                // An empty row has no ink, so the minimum advance is its full height.
                top += min_advance;
                continue;
            }
            let line_origin = egui::pos2(origin.x, top);
            let mut rendered =
                self.render_line(row, options, key, line_origin, pixels_per_point)?;
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
                let tree = self
                    .layout_row(src, &options.font_size_pt, options.display)
                    .map_err(map_error)?;
                let (shapes, rect) =
                    emit_shapes(&tree, &self.font, options, origin, pixels_per_point)
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
        let mut tree = self
            .layout_row(&prepared.latex, &options.font_size_pt, options.display)
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

/// Stretches bar delimiters to the height of the body they enclose.
///
/// The layout engine can only pick vertical MATH variants, and the embedded
/// face has none for [`BAR_GLYPHS`]: a `vmatrix` bar therefore stays near one em
/// while parentheses reach the matrix height. Replacing the glyph with a line
/// sized to the body keeps the preview, SVG and PNG output identical.
fn stretch_bar_delimiters(bx: &mut MathBox, font: &MathFont) {
    match &mut bx.content {
        BoxContent::HList(children) => {
            for child in children.iter_mut() {
                stretch_bar_delimiters(child, font);
            }
            stretch_bar_pair(children, font);
        }
        BoxContent::VList(children) | BoxContent::Overlap(children) => {
            for child in children.iter_mut() {
                stretch_bar_delimiters(child, font);
            }
        }
        BoxContent::Color(_, inner) | BoxContent::BackColor(_, inner) => {
            stretch_bar_delimiters(inner, font);
        }
        BoxContent::Frame { inner, .. } => stretch_bar_delimiters(inner, font),
        BoxContent::Empty
        | BoxContent::Kern(_)
        | BoxContent::Rule
        | BoxContent::Line { .. }
        | BoxContent::Glyph { .. } => {}
    }
}

/// Rewrites the enclosing bars of `[left, body, right]` when the body is taller.
///
/// Delimited bodies (matrix stacks and `\left…\right`) lay out as exactly three
/// siblings, so any other row shape is left untouched.
fn stretch_bar_pair(children: &mut [MathBox], font: &MathFont) {
    let [left, body, right] = children else {
        return;
    };
    let Some(bar) = bar_glyph(left) else {
        return;
    };
    if bar_glyph(right) != Some(bar) {
        return;
    }
    let height = (&body.height + &body.shift).clamp_nonneg();
    let depth = (&body.depth - &body.shift).clamp_nonneg();
    let body_span = &height + &depth;
    let bar_span = &left.height + &left.depth;
    if !matches!(body_span.cmp(&bar_span), Some(Ordering::Greater)) {
        return;
    }
    let Some(stretched) = stretched_bar(font, bar, &height, &depth) else {
        return;
    };
    *left = stretched.clone();
    *right = stretched;
}

/// The bar character when this box is a single unstretchable delimiter glyph.
fn bar_glyph(bx: &MathBox) -> Option<char> {
    match &bx.content {
        BoxContent::Glyph { ch, .. } if BAR_GLYPHS.contains(ch) => Some(*ch),
        _ => None,
    }
}

/// A stroked segment spanning `height`/`depth` that keeps the glyph's advance.
///
/// A filled rule would be equally accurate, but tiny-skia draws sub-pixel-wide
/// rectangles through its hairline path, which asserts on degenerate widths in
/// debug builds; a stroked segment has no such edge case.
fn stretched_bar(font: &MathFont, ch: char, height: &Dim, depth: &Dim) -> Option<MathBox> {
    let metrics = font.glyph(ch).ok()?;
    let units_per_em = font.units_per_em();
    let bbox = font
        .face()
        .glyph_bounding_box(ttf_parser::GlyphId(metrics.glyph_id))?;
    let ink_width =
        Dim::from_font_units(i64::from(bbox.x_max) - i64::from(bbox.x_min), units_per_em);
    if ink_width.is_zero() {
        return None;
    }
    let left_bearing = Dim::from_font_units(i64::from(bbox.x_min), units_per_em).clamp_nonneg();
    let center = &left_bearing + &(&ink_width / &Dim::from_i64(2));
    Some(MathBox {
        width: metrics.advance,
        height: height.clone(),
        depth: depth.clone(),
        italic: Dim::zero(),
        shift: Dim::zero(),
        content: BoxContent::Line {
            x1: center.clone(),
            y1: height.clone(),
            x2: center,
            y2: &Dim::zero() - depth,
            thickness: ink_width,
        },
    })
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

/// Vertical space that turns a row's ink extent into the preview's line advance.
///
/// The rule has no width, so SVG and PNG renderers skip it visually while
/// [`MathBox::vpack`] still accounts for its height between rows.
fn row_spacer(span: &Dim) -> MathBox {
    let advance = Dim::ratio(EXPORT_LINE_ADVANCE.0, EXPORT_LINE_ADVANCE.1);
    let gap = Dim::ratio(EXPORT_LINE_GAP.0, EXPORT_LINE_GAP.1);
    // Mirrors the preview's `(height + gap).max(min_advance)` advance.
    let height = &(span + &gap).max(&advance) - span;
    MathBox::rule(Dim::zero(), height, Dim::zero())
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
    use latex_rust::{BoxContent, Dim, MathBox};

    use super::{Renderer, BAR_GLYPHS};
    use crate::model::{source_rows, ExportStyle, RenderKey};

    fn preview(src: &str) -> crate::model::Rendered {
        let renderer = Renderer::new().expect("embedded font");
        let key = RenderKey::new(src, 20, false, 1.0, [0, 0, 0]);
        renderer
            .render(src, key, egui::pos2(0.0, 0.0))
            .expect("preview renders")
    }

    fn bar_spans(bx: &MathBox, spans: &mut Vec<Dim>) {
        match &bx.content {
            BoxContent::Line { y1, y2, .. } => spans.push(y1 - y2),
            BoxContent::HList(children)
            | BoxContent::VList(children)
            | BoxContent::Overlap(children) => {
                for child in children {
                    bar_spans(child, spans);
                }
            }
            BoxContent::Color(_, inner) | BoxContent::BackColor(_, inner) => {
                bar_spans(inner, spans);
            }
            BoxContent::Frame { inner, .. } => bar_spans(inner, spans),
            _ => {}
        }
    }

    fn bar_glyph_count(bx: &MathBox) -> usize {
        match &bx.content {
            BoxContent::Glyph { ch, .. } if BAR_GLYPHS.contains(ch) => 1,
            BoxContent::HList(children)
            | BoxContent::VList(children)
            | BoxContent::Overlap(children) => children.iter().map(bar_glyph_count).sum(),
            BoxContent::Color(_, inner) | BoxContent::BackColor(_, inner) => bar_glyph_count(inner),
            BoxContent::Frame { inner, .. } => bar_glyph_count(inner),
            _ => 0,
        }
    }

    #[test]
    fn vmatrix_bars_span_the_matrix_height() {
        let renderer = Renderer::new().expect("embedded font");
        let tree = renderer
            .layout_export(
                r"\begin{vmatrix} a & b \\ c & d \end{vmatrix}",
                &ExportStyle::default(),
            )
            .expect("matrix lays out");
        let mut spans = Vec::new();
        bar_spans(&tree, &mut spans);
        let tallest = spans
            .into_iter()
            .max_by(|left, right| left.cmp(right).expect("finite dimensions"))
            .expect("bars become segments");
        assert!(
            tallest.eq_dim(&(&tree.height + &tree.depth)),
            "a vmatrix bar must reach the matrix height"
        );
        assert_eq!(bar_glyph_count(&tree), 0, "bars are drawn as segments");
    }

    #[test]
    fn leaves_unpaired_bars_as_glyphs() {
        let renderer = Renderer::new().expect("embedded font");
        let tree = renderer
            .layout_export("a | b", &ExportStyle::default())
            .expect("row lays out");
        assert_eq!(bar_glyph_count(&tree), 1, "a lone bar is not a delimiter");
    }

    #[test]
    fn splits_hard_newlines_only_at_depth_zero() {
        let source =
            "a + b\n\\frac{x\n}{y}\n\\begin{pmatrix}\n1 & 2 \\\\\n3 & 4\n\\end{pmatrix}\nc";
        assert_eq!(
            source_rows(source),
            vec![
                "a + b",
                "\\frac{x\n}{y}",
                "\\begin{pmatrix}\n1 & 2 \\\\\n3 & 4\n\\end{pmatrix}",
                "c",
            ]
        );
    }

    #[test]
    fn blank_lines_advance_the_preview() {
        let compact = preview("x\ny");
        let spaced = preview("x\n\ny");
        assert!(
            spaced.rect.height() > compact.rect.height(),
            "a blank line must add vertical space"
        );
    }

    #[test]
    fn blank_lines_add_export_space() {
        let renderer = Renderer::new().expect("embedded font");
        let style = ExportStyle::default();
        let compact = renderer.layout_export("x\ny", &style).expect("stacked");
        let spaced = renderer.layout_export("x\n\ny", &style).expect("spaced");
        let compact_span = &compact.height + &compact.depth;
        let spaced_span = &spaced.height + &spaced.depth;
        assert!(
            matches!(
                spaced_span.cmp(&compact_span),
                Some(std::cmp::Ordering::Greater)
            ),
            "a blank line must add export space"
        );
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
