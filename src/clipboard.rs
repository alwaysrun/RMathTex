//! Single-transaction clipboard writer and the Office read-back chain.

use clipboard_win::{raw, Clipboard};

use crate::convert;
use crate::model::{
    normalize_source, AppError, CF_HTML_FORMAT_NAME, CF_UNICODETEXT, CLIPBOARD_ATTEMPTS,
    MATHML_FORMAT_NAMES, MSG_CLIPBOARD_BUSY, MSG_NO_FORMULA,
};

/// Writes MathML, CF_HTML(OMML) and plain LaTeX in one clipboard transaction.
///
/// Every format is attempted even when an earlier one fails, because Office
/// negotiates whichever format it prefers. Returns the formats actually stored.
pub fn write_office(
    latex: &str,
    mathml: Option<&str>,
    omml: Option<&str>,
) -> Result<Vec<&'static str>, AppError> {
    let _clipboard = open()?;
    raw::empty().map_err(|error| AppError::caused("E403", "剪贴板清空失败", error))?;
    let mut written = Vec::new();
    if let Some(mathml) = mathml {
        for name in MATHML_FORMAT_NAMES {
            if let Ok(format) = format_code(name) {
                if raw::set_without_clear(format.get(), mathml.as_bytes()).is_ok() {
                    written.push(name);
                }
            }
        }
    }
    if let Some(omml) = omml {
        if let Ok(format) = format_code(CF_HTML_FORMAT_NAME) {
            let html = cf_html(&wrap_omml_paragraph(omml));
            if raw::set_without_clear(format.get(), &html).is_ok() {
                written.push(CF_HTML_FORMAT_NAME);
            }
        }
    }
    raw::set_without_clear(CF_UNICODETEXT, &utf16le_with_nul(latex))
        .map_err(|error| AppError::caused("E403", "纯文本写入失败", error))?;
    written.push("CF_UNICODETEXT");
    Ok(written)
}

/// Replaces the clipboard with plain text; used by "copy as Markdown".
pub fn write_text(text: &str) -> Result<(), AppError> {
    let _clipboard = open()?;
    raw::empty().map_err(|error| AppError::caused("E403", "剪贴板清空失败", error))?;
    raw::set_without_clear(CF_UNICODETEXT, &utf16le_with_nul(text))
        .map_err(|error| AppError::caused("E403", "纯文本写入失败", error))
}

/// Reads a formula copied from Word: CF_HTML/OMML first, plain text last.
pub fn read_office() -> Result<String, AppError> {
    let _clipboard = open()?;
    if let Ok(format) = format_code(CF_HTML_FORMAT_NAME) {
        if raw::is_format_avail(format.get()) {
            let mut buffer = Vec::new();
            if raw::get_vec(format.get(), &mut buffer).is_ok() {
                if let Some(omml) = extract_omml(&buffer) {
                    if let Some(latex) = omml_subtree_to_latex(&omml) {
                        return Ok(normalize_source(&latex).body);
                    }
                }
            }
        }
    }
    if has_mathml() {
        return Err(AppError::new("E405", MSG_NO_FORMULA));
    }
    let mut buffer = Vec::new();
    if raw::is_format_avail(CF_UNICODETEXT) && raw::get_vec(CF_UNICODETEXT, &mut buffer).is_ok() {
        let text = utf16_bytes_to_string(&buffer);
        let trimmed = text.trim_matches(|c: char| c.is_whitespace() || c == '\0');
        if !trimmed.is_empty() && (trimmed.contains('\\') || trimmed.contains('$')) {
            return Ok(normalize_source(trimmed).body);
        }
    }
    Err(AppError::new("E405", MSG_NO_FORMULA))
}

/// Builds a CF_HTML payload with byte-accurate, zero-padded offsets.
#[must_use]
pub fn cf_html(fragment: &str) -> Vec<u8> {
    const PRE_HTML: &str = "<html xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\"><body><!--StartFragment-->";
    const POST_HTML: &str = "<!--EndFragment--></body></html>";
    const FIELD_WIDTH: usize = 10;
    let header_len = "Version:0.9\r\n".len()
        + "StartHTML:".len()
        + FIELD_WIDTH
        + "\r\nEndHTML:".len()
        + FIELD_WIDTH
        + "\r\nStartFragment:".len()
        + FIELD_WIDTH
        + "\r\nEndFragment:".len()
        + FIELD_WIDTH
        + "\r\n".len();
    let body = format!("\n{fragment}\n");
    let start_html = header_len;
    let start_fragment = start_html + PRE_HTML.len();
    let end_fragment = start_fragment + body.len();
    let end_html = end_fragment + POST_HTML.len();
    format!(
        "Version:0.9\r\nStartHTML:{start_html:010}\r\nEndHTML:{end_html:010}\
         \r\nStartFragment:{start_fragment:010}\r\nEndFragment:{end_fragment:010}\r\n\
         {PRE_HTML}{body}{POST_HTML}"
    )
    .into_bytes()
}

/// Extracts the first balanced `m:oMathPara` or `m:oMath` subtree.
#[must_use]
pub fn extract_omml(html: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(html);
    extract_element(&text, "m:oMathPara")
        .or_else(|| extract_element(&text, "m:oMath"))
        .map(|element| strip_conditional_wrapper(&element))
}

/// Encodes text as NUL-terminated UTF-16LE, the `CF_UNICODETEXT` layout.
#[must_use]
pub fn utf16le_with_nul(text: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity((text.len() + 1) * 2);
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes
}

fn open() -> Result<Clipboard, AppError> {
    Clipboard::new_attempts(CLIPBOARD_ATTEMPTS)
        .map_err(|error| AppError::caused("E401", MSG_CLIPBOARD_BUSY, error))
}

fn format_code(name: &str) -> Result<std::num::NonZeroU32, AppError> {
    raw::register_format(name)
        .ok_or_else(|| AppError::new("E402", format!("剪贴板格式注册失败：{name}")))
}

fn has_mathml() -> bool {
    MATHML_FORMAT_NAMES
        .iter()
        .any(|name| format_code(name).is_ok_and(|format| raw::is_format_avail(format.get())))
}

fn wrap_omml_paragraph(omml: &str) -> String {
    format!("<m:oMathPara>{omml}</m:oMathPara>")
}

/// Tries the extracted subtree, then its inner `m:oMath`, since the converter
/// only accepts a single formula element.
fn omml_subtree_to_latex(omml: &str) -> Option<String> {
    if let Ok(latex) = convert::from_omml(omml) {
        return Some(latex);
    }
    let inner = extract_element(omml, "m:oMath")?;
    convert::from_omml(&inner).ok()
}

fn strip_conditional_wrapper(element: &str) -> String {
    element
        .replace("<!--[if gte mso 9]>", "")
        .replace("<!--[if gte mso 9]><xml>", "")
        .replace("<![endif]-->", "")
        .replace("</xml>", "")
}

fn extract_element(text: &str, name: &str) -> Option<String> {
    let open = format!("<{name}");
    let close = format!("</{name}>");
    let start = find_open(text, &open, 0)?;
    let mut depth = 0usize;
    let mut cursor = start;
    loop {
        let next_open = find_open(text, &open, cursor).unwrap_or(usize::MAX);
        let next_close = text[cursor..]
            .find(&close)
            .map_or(usize::MAX, |offset| cursor + offset);
        if next_close == usize::MAX {
            return None;
        }
        if next_open < next_close {
            depth += 1;
            cursor = next_open + open.len();
        } else {
            depth -= 1;
            if depth == 0 {
                return Some(text[start..next_close + close.len()].to_owned());
            }
            cursor = next_close + close.len();
        }
    }
}

fn find_open(text: &str, open: &str, from: usize) -> Option<usize> {
    let mut search = from;
    while let Some(found) = text[search..].find(open) {
        let start = search + found;
        let after = &text[start + open.len()..];
        let boundary = after
            .chars()
            .next()
            .is_some_and(|c| c == '>' || c.is_whitespace());
        if boundary && !after.starts_with("/>") {
            return Some(start);
        }
        search = start + open.len();
    }
    None
}

fn utf16_bytes_to_string(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}
