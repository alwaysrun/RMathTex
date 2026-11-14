//! SVG / PNG / docx generation and the native save transaction.

use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use latex_rust::{
    render_png, render_svg, Color, Dim, MathBox, MathFont, PngBackground, PngOptions, SvgOptions,
};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::convert;
use crate::model::{AppError, ExportStyle, ExportTarget, PngBackgroundChoice, DPI_MAX};

/// `[Content_Types].xml` — one document part, no styles, no macros.
const CONTENT_TYPES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
<Default Extension=\"xml\" ContentType=\"application/xml\"/>\
<Override PartName=\"/word/document.xml\" \
ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
</Types>";

/// `_rels/.rels` — points at the single document part.
const ROOT_RELS: &str =
    "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" \
Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" \
Target=\"word/document.xml\"/></Relationships>";

/// Renders a laid-out tree as a self-contained SVG with outline glyphs.
///
/// The tree is laid out by `render::Renderer::layout_export` so the exported
/// delimiters match the preview.
pub fn svg(tree: &MathBox, style: &ExportStyle, font: &MathFont) -> Result<String, AppError> {
    let mut options = SvgOptions::new();
    options.font_size_pt = Dim::from_i64(style.font_size_pt);
    options.color = Color::rgb(style.color[0], style.color[1], style.color[2]);
    options.display = style.display;
    render_svg(tree, font, &options)
        .map_err(|error| AppError::caused("E601", "SVG 渲染失败", error))
}

/// Rasterizes a laid-out tree; the resolution must stay inside `(0, DPI_MAX]`.
pub fn png(tree: &MathBox, style: &ExportStyle, font: &MathFont) -> Result<Vec<u8>, AppError> {
    if !(1..=DPI_MAX).contains(&style.dpi) {
        return Err(AppError::new("E601", "PNG DPI 越界"));
    }
    let mut options = PngOptions::new();
    options.font_size_pt = Dim::from_i64(style.font_size_pt);
    options.dpi = Dim::from_i64(style.dpi);
    options.color = Color::rgb(style.color[0], style.color[1], style.color[2]);
    options.background = match style.background {
        PngBackgroundChoice::Transparent => PngBackground::Transparent,
        PngBackgroundChoice::White => PngBackground::White,
    };
    options.display = style.display;
    render_png(tree, font, &options)
        .map_err(|error| AppError::caused("E601", "PNG 渲染失败", error))
}

/// Packages a minimal OPC document whose only content is editable formulas.
///
/// Each entry becomes one paragraph; `None` keeps a blank source line as an
/// empty paragraph so the vertical gap survives the export.
pub fn docx(rows: &[Option<String>], display: bool) -> Result<Vec<u8>, AppError> {
    let document = document_xml(rows, display);
    opc_zip(&[
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        ("_rels/.rels", ROOT_RELS.as_bytes()),
        ("word/document.xml", document.as_bytes()),
    ])
}

/// Asks for a destination path and writes atomically; `Ok(None)` means cancelled.
pub fn save(target: ExportTarget, bytes: &[u8]) -> Result<Option<PathBuf>, AppError> {
    let (filter_name, extension) = target_filter(target);
    let choice = rfd::FileDialog::new()
        .add_filter(filter_name, &[extension])
        .set_file_name(target_file_name(target))
        .save_file();
    let Some(path) = choice else {
        return Ok(None);
    };
    atomic_write(&path, bytes)?;
    Ok(Some(path))
}

/// Display name used by the status bar (`{target}` placeholder).
#[must_use]
pub fn target_label(target: ExportTarget) -> &'static str {
    match target {
        ExportTarget::Svg => "SVG",
        ExportTarget::Png => "PNG",
        ExportTarget::Docx => "DOCX",
        ExportTarget::Markdown => "Markdown",
    }
}

fn target_filter(target: ExportTarget) -> (&'static str, &'static str) {
    match target {
        ExportTarget::Svg => ("SVG 图像", "svg"),
        ExportTarget::Png => ("PNG 图像", "png"),
        ExportTarget::Docx => ("Word 文档", "docx"),
        ExportTarget::Markdown => ("Markdown 片段", "md"),
    }
}

fn target_file_name(target: ExportTarget) -> &'static str {
    match target {
        ExportTarget::Svg => "formula.svg",
        ExportTarget::Png => "formula.png",
        ExportTarget::Docx => "formula.docx",
        ExportTarget::Markdown => "formula.md",
    }
}

/// Builds `word/document.xml`; `display` alone decides the paragraph wrapper.
///
/// A `None` row becomes `<w:p/>`: Word renders it as the blank line the source
/// contained, without inventing formula content.
#[must_use]
pub fn document_xml(rows: &[Option<String>], display: bool) -> String {
    let mut body = String::new();
    for row in rows {
        let Some(omml) = row else {
            body.push_str("<w:p/>");
            continue;
        };
        let inner = convert::omml_inner(omml);
        let paragraph = if display {
            format!("<w:p><m:oMathPara><m:oMath>{inner}</m:oMath></m:oMathPara></w:p>")
        } else {
            format!("<w:p><m:oMath>{inner}</m:oMath></w:p>")
        };
        body.push_str(&paragraph);
    }
    format!(
        "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
         xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\">\
         <w:body>{body}<w:sectPr/></w:body></w:document>"
    )
}

fn opc_zip(parts: &[(&str, &[u8])]) -> Result<Vec<u8>, AppError> {
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in parts {
        writer
            .start_file(*name, options)
            .map_err(|error| AppError::caused("E603", "OPC 打包失败", error))?;
        writer
            .write_all(bytes)
            .map_err(|error| AppError::caused("E603", "OPC 打包失败", error))?;
    }
    writer
        .finish()
        .map(|cursor| cursor.into_inner())
        .map_err(|error| AppError::caused("E603", "OPC 打包失败", error))
}

/// Writes to a sibling temporary file, then renames over the destination.
pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("formula");
    let temporary = path.with_file_name(format!(".{file_name}.{}.{stamp}.tmp", std::process::id()));
    fs::write(&temporary, bytes).map_err(|error| AppError::caused("E605", "写文件失败", error))?;
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(AppError::caused("E605", "写文件失败", error));
    }
    Ok(())
}
