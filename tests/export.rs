//! File export contract tests: SVG, PNG, docx OPC package, Markdown fragment.

// The `src` modules are compiled per test binary; items used only by the app
// are dead code here.
#![allow(dead_code)]

#[path = "../src/convert.rs"]
mod convert;
#[path = "../src/export.rs"]
mod export;
#[path = "../src/model.rs"]
mod model;
#[path = "../src/render.rs"]
mod render;

use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use export::atomic_write;
use model::{Delimiter, ExportStyle, EXPORT_COLOR};
use render::Renderer;

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

fn renderer() -> Renderer {
    Renderer::new().expect("font")
}

fn style() -> ExportStyle {
    ExportStyle::default()
}

#[test]
fn svg_is_self_contained() {
    let renderer = renderer();
    let svg = export::svg("\\frac{a}{b}", &style(), renderer.font()).expect("svg");
    // The serializer may emit an XML declaration before the root element.
    assert!(svg.contains("<svg"), "{svg}");
    assert!(svg.trim_end().ends_with("</svg>"), "{svg}");
    assert!(svg.contains("xmlns=\"http://www.w3.org/2000/svg\""));
    assert!(svg.contains("<path"), "{svg}");
    assert!(!svg.contains("<text"), "glyphs must be outlines");
    assert!(!svg.contains("xlink:href"), "no external references");
    assert!(!svg.contains("@font-face"), "no embedded font");
    assert!(!svg.contains("<image"), "no raster payload");
    assert!(svg.contains("fill=\"#000000\""), "fixed black fill");
}

#[test]
fn png_header_and_alpha() {
    let renderer = renderer();
    let bytes = export::png("x^2", &style(), renderer.font()).expect("png");
    assert_eq!(&bytes[..8], &PNG_SIGNATURE);
    assert_eq!(&bytes[12..16], b"IHDR");
    let width = u32::from_be_bytes(bytes[16..20].try_into().expect("width"));
    let height = u32::from_be_bytes(bytes[20..24].try_into().expect("height"));
    let bit_depth = bytes[24];
    let color_type = bytes[25];
    assert!(width > 0 && height > 0, "{width}x{height}");
    assert_eq!(bit_depth, 8);
    assert_eq!(color_type, 6, "RGBA is required for transparent background");
    let mut out_of_range = style();
    out_of_range.dpi = 0;
    let error =
        export::png("x^2", &out_of_range, renderer.font()).expect_err("dpi must be checked");
    assert_eq!(error.code, "E601");
}

#[test]
fn docx_zip_parts_and_omml() {
    let omml = convert::to_omml("\\frac{a}{b}").expect("omml");
    let bytes = export::docx(&omml, true).expect("docx");
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("zip");
    let mut names: Vec<String> = archive
        .file_names()
        .map(|name| name.expect("entry name").into_owned())
        .collect();
    names.sort();
    assert_eq!(
        names,
        ["[Content_Types].xml", "_rels/.rels", "word/document.xml"]
    );
    let mut document = String::new();
    archive
        .by_name("word/document.xml")
        .expect("document part")
        .read_to_string(&mut document)
        .expect("read document");
    assert!(document.contains("<w:document"), "{document}");
    assert!(document.contains("<m:oMathPara><m:oMath>"), "{document}");
    assert!(document.contains("</m:oMath></m:oMathPara>"), "{document}");
    assert!(
        !document.contains("&lt;m:oMath"),
        "OMML must not be escaped"
    );
    assert!(!document.contains("<w:drawing"), "must not be an image");
}

#[test]
fn export_style_theme_independent() {
    let renderer = renderer();
    let defaults = style();
    assert_eq!(defaults.color, EXPORT_COLOR);
    assert_eq!(defaults.font_size_pt, model::DEFAULT_EXPORT_FONT_SIZE_PT);
    let first = export::svg("x^2", &defaults, renderer.font()).expect("svg");
    let second = export::svg("x^2", &defaults, renderer.font()).expect("svg");
    assert_eq!(first, second, "export output must be deterministic");
    assert!(first.contains("fill=\"#000000\""), "fixed black fill");
    let markdown = model::to_markdown("x^2", Delimiter::Inline);
    assert_eq!(markdown, "$x^2$");
    assert!(!markdown.contains("fill="));
    assert!(!markdown.contains('<'));
}

#[test]
fn markdown_file_is_single_formula_fragment() {
    let text = model::to_markdown("\\frac{a}{b}", Delimiter::Display);
    assert_eq!(text, "$$\n\\frac{a}{b}\n$$");
    let bytes = text.as_bytes();
    assert!(!bytes.starts_with(&[0xEF, 0xBB, 0xBF]), "no BOM");
    assert_eq!(
        text.lines().count(),
        3,
        "opening and closing $$ on their own lines"
    );
    assert!(!text.contains("# "), "no document headings");
    assert!(!text.starts_with("<!DOCTYPE"), "plain fragment, not HTML");
}

#[test]
fn save_cancel_writes_nothing() {
    let renderer = renderer();
    let directory = unique_directory();
    let target = directory.join("formula.svg");
    // Producing payloads never touches the file system.
    let svg = export::svg("x^2", &style(), renderer.font()).expect("svg");
    let png = export::png("x^2", &style(), renderer.font()).expect("png");
    let omml = convert::to_omml("x^2").expect("omml");
    let docx = export::docx(&omml, false).expect("docx");
    assert!(!target.exists(), "payload generation must not write files");
    assert!(svg.contains("<svg") && !png.is_empty() && !docx.is_empty());
    // The atomic writer is the only writer and leaves no temporary behind.
    atomic_write(&target, svg.as_bytes()).expect("write");
    assert_eq!(fs::read_to_string(&target).expect("read"), svg);
    assert!(
        temporary_files(&directory).is_empty(),
        "temporary file leaked"
    );
    let missing = directory.join("missing").join("formula.svg");
    let error = atomic_write(&missing, b"x").expect_err("missing directory must fail");
    assert_eq!(error.code, "E605");
    assert!(
        temporary_files(&directory).is_empty(),
        "temporary file leaked"
    );
    fs::remove_dir_all(&directory).expect("cleanup");
}

fn temporary_files(directory: &Path) -> Vec<PathBuf> {
    fs::read_dir(directory)
        .expect("read dir")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "tmp"))
        .collect()
}

fn unique_directory() -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "rmath-export-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos())
    ));
    fs::create_dir_all(&directory).expect("create temp dir");
    directory
}
