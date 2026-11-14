//! Round-trip and byte-level contract tests (Markdown, OMML, CF_HTML).

// The `src` modules are compiled per test binary; items used only by the app
// are dead code here.
#![allow(dead_code)]

#[path = "../src/clipboard.rs"]
mod clipboard;
#[path = "../src/convert.rs"]
mod convert;
#[path = "../src/export.rs"]
mod export;
#[path = "../src/linearmath.rs"]
mod linearmath;
#[path = "../src/model.rs"]
mod model;
#[path = "../src/render.rs"]
mod render;

use eframe::egui;

use model::{Delimiter, NormalizedSource, RenderKey};
use render::Renderer;

const OMML_FORMULAS: &[&str] = &[
    "\\frac{a}{b}",
    "x_{i}^{j}",
    "\\sqrt[3]{x}",
    "\\sum_{i=1}^{n} i",
    "\\begin{bmatrix} 1 & 2 \\\\ 3 & 4 \\end{bmatrix}",
    "\\int_{0}^{\\infty} e^{-x^{2}}\\,\\mathrm{d}x",
    "\\left(\\frac{a}{b}\\right)",
    "\\alpha\\beta\\gamma",
    "\\mathbb{R}",
    "\\lim_{x\\to 0}\\frac{\\sin x}{x}",
];

fn roundtrip(text: &str) -> NormalizedSource {
    model::normalize_source(text)
}

#[test]
fn markdown_roundtrip_all_flavors() {
    let bodies = [
        "\\frac{a}{b}",
        "x^2",
        "a \\$ b",
        "\\begin{matrix} 1 & 2 \\end{matrix}",
    ];
    let flavors = [
        Delimiter::None,
        Delimiter::Inline,
        Delimiter::Display,
        Delimiter::MacroInline,
        Delimiter::MacroDisplay,
        Delimiter::Fenced,
    ];
    for body in bodies {
        for flavor in flavors {
            let markdown = model::to_markdown(body, flavor);
            let normalized = roundtrip(&markdown);
            assert_eq!(
                normalized.body, body,
                "body changed for {flavor:?} on {body:?} ({markdown:?})"
            );
            assert_eq!(
                normalized.delimiter, flavor,
                "delimiter changed for {flavor:?} on {body:?} ({markdown:?})"
            );
            assert_eq!(normalized.raw, markdown);
            assert!(
                normalized.warning.is_none(),
                "unexpected warning on {markdown:?}"
            );
        }
    }
}

#[test]
fn omml_roundtrip_core_set() {
    for formula in OMML_FORMULAS {
        let first =
            convert::to_omml(formula).unwrap_or_else(|error| panic!("to_omml {formula}: {error}"));
        assert!(first.contains("<m:oMath"), "{formula} -> {first}");
        let latex = convert::from_omml(&first)
            .unwrap_or_else(|error| panic!("from_omml {formula}: {error}"));
        assert!(!latex.trim().is_empty(), "{formula} produced empty LaTeX");
        let second = convert::to_omml(&latex)
            .unwrap_or_else(|error| panic!("to_omml (second) {formula}: {error}"));
        let third = convert::from_omml(&second)
            .and_then(|again| convert::to_omml(&again))
            .unwrap_or_else(|error| panic!("stable roundtrip {formula}: {error}"));
        assert_eq!(second, third, "{formula} does not reach a stable OMML form");
    }
}

#[test]
fn omml_roundtrip_with_normalization() {
    let normalized = roundtrip("$\\frac{1}{2}$");
    assert_eq!(normalized.body, "\\frac{1}{2}");
    let omml = convert::to_omml(&normalized.body).expect("to_omml");
    let latex = convert::from_omml(&omml).expect("from_omml");
    assert_eq!(roundtrip(&latex).body, normalized.body);
}

#[test]
fn shapes_nonempty_and_bbox() {
    let renderer = Renderer::new().expect("font");
    let origin = egui::Pos2::new(12.0, 24.0);
    let key = RenderKey::new("\\frac{a}{b}", 14, true, 1.0, [0, 0, 0]);
    let rendered = renderer
        .render("\\frac{a}{b}", key, origin)
        .expect("render");
    assert!(!rendered.shapes.is_empty(), "no shapes produced");
    assert!(rendered.rect.width() > 0.0, "empty width");
    assert!(rendered.rect.height() > 0.0, "empty height");
    assert!(rendered.rect.min.x >= origin.x - 1.0);
    assert_eq!(rendered.key, key);
}

#[test]
fn cf_html_offsets() {
    let fragment = "<m:oMathPara><m:oMath><m:r><m:t>x</m:t></m:r></m:oMath></m:oMathPara>";
    let bytes = clipboard::cf_html(fragment);
    let text = String::from_utf8(bytes).expect("utf-8");
    let start_html = header_field(&text, "StartHTML:");
    let end_html = header_field(&text, "EndHTML:");
    let start_fragment = header_field(&text, "StartFragment:");
    let end_fragment = header_field(&text, "EndFragment:");
    assert_eq!(end_html, text.len());
    assert!(text[start_html..].starts_with("<html xmlns:m="));
    assert!(text[start_html..].contains("<body><!--StartFragment-->"));
    assert_eq!(
        &text[start_fragment..end_fragment],
        format!("\n{fragment}\n")
    );
    assert!(text[start_html..start_fragment].ends_with("<!--StartFragment-->"));
    assert!(text[end_fragment..end_html].starts_with("<!--EndFragment-->"));
    for key in ["StartHTML:", "EndHTML:", "StartFragment:", "EndFragment:"] {
        let digits = &text[text.find(key).expect("field") + key.len()..][..10];
        assert!(
            digits.chars().all(|c| c.is_ascii_digit()),
            "{key} not zero padded"
        );
    }
}

#[test]
fn extract_omml_conditional_comment() {
    let html = "<html><body><!--StartFragment--><p><img src=\"formula.wmz\"/></p>\
                <!--[if gte mso 9]><xml><m:oMathPara><m:oMath><m:r><m:t>x</m:t></m:r>\
                </m:oMath></m:oMathPara></xml><![endif]-->\
                <!--EndFragment--></body></html>";
    let omml = clipboard::extract_omml(html.as_bytes()).expect("omml");
    assert!(omml.starts_with("<m:oMathPara>"), "{omml}");
    assert!(omml.ends_with("</m:oMathPara>"), "{omml}");
    assert!(!omml.contains("[if gte mso 9]"), "{omml}");
    assert!(!omml.contains("<![endif]"), "{omml}");
    assert!(omml.contains("<m:oMath>"), "{omml}");
}

#[test]
fn markdown_output_three_paths_consistent() {
    use convert::{from_omml, to_omml};

    let body = "\\frac{a}{b}";
    let flavor = Delimiter::Display;
    // F12: copy as Markdown.
    let copied = model::to_markdown(body, flavor);
    // F13: exported fragment bytes.
    let exported = model::to_markdown(body, flavor).into_bytes();
    // F4 then F12: formula comes back from Office, then the user takes it away.
    let imported = to_omml(body)
        .and_then(|omml| from_omml(&omml))
        .expect("omml chain");
    let imported_body = roundtrip(&imported).body;
    let after_import = model::to_markdown(&imported_body, flavor);
    assert_eq!(copied, after_import);
    assert_eq!(exported, copied.as_bytes());
    assert_eq!(copied, "$$\n\\frac{a}{b}\n$$");
}

#[test]
fn word_linear_equation_array_converts_and_renders() {
    let latex = linearmath::to_latex("█(a&=b+c@&=d+e)").expect("equation array");
    assert_eq!(latex, "\\begin{aligned}a&=b+c\\\\&=d+e\\end{aligned}");
    let renderer = Renderer::new().expect("font");
    let key = RenderKey::new(&latex, 14, true, 1.0, [0, 0, 0]);
    renderer
        .render(&latex, key, egui::Pos2::ZERO)
        .expect("render imported formula");
}

#[test]
fn word_linear_matrix_radical_and_symbols() {
    assert_eq!(
        linearmath::to_latex("■(1&2@3&4)").expect("matrix"),
        "\\begin{pmatrix}1&2\\\\3&4\\end{pmatrix}"
    );
    assert_eq!(
        linearmath::to_latex("█(a=b@c=d)").expect("gather"),
        "\\begin{gather}a=b\\\\c=d\\end{gather}"
    );
    assert_eq!(
        linearmath::to_latex("α≤β+√(x)+√[3](y)").expect("symbols"),
        "\\alpha\\le\\beta+\\sqrt{x}+\\sqrt[3]{y}"
    );
    assert_eq!(
        linearmath::to_latex("x_(n+1)^(2)").expect("scripts"),
        "x_{n+1}^{2}"
    );
}

#[test]
fn latex_and_plain_pastes_are_not_rewritten() {
    assert!(linearmath::to_latex("\\frac{a}{b}").is_none());
    assert!(linearmath::to_latex("x^2 + y_1").is_none());
    assert!(linearmath::to_latex("plain prose").is_none());
    assert!(linearmath::has_equation_array("█(a&=b@c&=d)"));
    assert!(!linearmath::has_equation_array("■(a&b@c&d)"));
}

#[test]
fn omml_equation_array_reads_back() {
    let omml = "<m:oMathPara><m:oMath><m:eqArr>\
                <m:e><m:r><m:t>a</m:t></m:r><m:r><m:t>=</m:t></m:r>\
                <m:r><m:t>b</m:t></m:r></m:e>\
                <m:e><m:r><m:t>=</m:t></m:r><m:r><m:t>d</m:t></m:r></m:e>\
                </m:eqArr></m:oMath></m:oMathPara>";
    let latex = convert::from_omml(omml).expect("from_omml");
    assert_eq!(latex, "\\begin{array}{c}a=b\\\\=d\\end{array}");
}

fn header_field(text: &str, key: &str) -> usize {
    let start = text.find(key).expect("header field") + key.len();
    text[start..start + 10].parse().expect("numeric field")
}
