//! Data model, unified error type and the Markdown normalize/package pair.
//!
//! The module holds no I/O and no LaTeX parsing: it is the single place where
//! the normalized formula body, the `Delimiter` double meaning and the status
//! bar vocabulary live, so every other module can share one vocabulary.

use std::hash::{Hash, Hasher};
use std::ops::RangeInclusive;

// ----------------------------------------------------------------- constants

/// Debounce delay before the preview is re-rendered (milliseconds).
pub const DEBOUNCE_MS: u64 = 80;
/// Maximum accepted source size in bytes; larger input fails with E101.
pub const MAX_SOURCE_BYTES: usize = 65_536;
/// Maximum accepted brace nesting depth; deeper input fails with E102.
pub const MAX_NESTING_DEPTH: usize = 64;
/// Clipboard open attempts per transaction.
pub const CLIPBOARD_ATTEMPTS: usize = 10;
/// Default preview font size (points).
pub const DEFAULT_FONT_SIZE_PT: i64 = 14;
/// Default export font size (points).
pub const DEFAULT_EXPORT_FONT_SIZE_PT: i64 = 12;
/// Allowed preview font size range (inclusive, step 1).
pub const FONT_SIZE_RANGE: RangeInclusive<i64> = 10..=32;
/// Default PNG resolution.
pub const DEFAULT_DPI: i64 = 144;
/// Selectable PNG resolutions.
pub const DPI_CHOICES: [i64; 3] = [144, 300, 600];
/// Hard DPI ceiling, shared with the raster backend.
pub const DPI_MAX: i64 = 2400;
/// Fixed export glyph colour: exports never follow the UI theme.
pub const EXPORT_COLOR: [u8; 3] = [0, 0, 0];
/// Default PNG background.
pub const DEFAULT_BACKGROUND: PngBackgroundChoice = PngBackgroundChoice::Transparent;
/// Initial window size.
pub const WINDOW_SIZE: [f32; 2] = [960.0, 640.0];
/// Minimum window size.
pub const MIN_WINDOW_SIZE: [f32; 2] = [720.0, 480.0];
/// Row count of the source editor.
pub const EDITOR_ROWS: usize = 4;
/// Vertical margin egui puts around a multiline `TextEdit` (2pt top + 2pt bottom).
pub const TEXT_EDIT_MARGIN_Y: f32 = 4.0;
/// Share of the workspace height the source editor occupies by default.
pub const SOURCE_PANEL_FRACTION: f32 = 1.0 / 3.0;
/// `CF_UNICODETEXT` clipboard format id.
pub const CF_UNICODETEXT: u32 = 13;
/// CF_HTML clipboard format name.
pub const CF_HTML_FORMAT_NAME: &str = "HTML Format";
/// Registered MathML clipboard format names, primary first.
pub const MATHML_FORMAT_NAMES: [&str; 2] = ["MathML", "MathML Presentation"];
/// Flavor used when the body carries no delimiter and the user follows input.
pub const AUTO_FLAVOR_FALLBACK: Delimiter = Delimiter::Inline;
/// Opening line of a GitHub math fence.
pub const MD_FENCE_OPEN: &str = "```math\n";
/// Closing line of a GitHub math fence.
pub const MD_FENCE_CLOSE: &str = "\n```";
/// Configuration file name, stored next to the executable.
pub const CONFIG_FILE_NAME: &str = "RMathTex.json";
/// Schema version written to the configuration file.
pub const CONFIG_VERSION: u32 = 1;

/// Prefix/suffix table of [`to_markdown`]; the six rows are the format contract.
pub const MD_DELIMITERS: [(Delimiter, &str, &str); 6] = [
    (Delimiter::None, "", ""),
    (Delimiter::Inline, "$", "$"),
    (Delimiter::Display, "$$\n", "\n$$"),
    (Delimiter::MacroInline, "\\(", "\\)"),
    (Delimiter::MacroDisplay, "\\[", "\\]"),
    (Delimiter::Fenced, MD_FENCE_OPEN, MD_FENCE_CLOSE),
];

/// Windows CJK font candidates for the UI, most preferred first.
///
/// The fonts bundled with egui carry no CJK glyphs, so every Chinese label would
/// render as a tofu box without one of these system fallbacks.
pub const UI_FONT_CANDIDATES: [(&str, &str); 5] = [
    ("Microsoft YaHei", "C:\\Windows\\Fonts\\msyh.ttc"),
    ("Microsoft YaHei Bold", "C:\\Windows\\Fonts\\msyhbd.ttc"),
    ("SimHei", "C:\\Windows\\Fonts\\simhei.ttf"),
    ("SimSun", "C:\\Windows\\Fonts\\simsun.ttc"),
    ("DengXian", "C:\\Windows\\Fonts\\Deng.ttf"),
];

/// Windows symbol font candidates for the UI, most preferred first.
///
/// The CJK fallbacks above lack several mathematical operators and arrows used
/// by the formula labels (`∓`, `∮`, `⟹`, `⟶`, …), so a symbol font is appended
/// to the same fallback chain. Every candidate is a single-face `.ttf` because
/// egui panics when a font collection index is out of range.
pub const UI_SYMBOL_FONT_CANDIDATES: [(&str, &str); 2] = [
    ("Segoe UI Symbol", "C:\\Windows\\Fonts\\seguisym.ttf"),
    ("Lucida Sans Unicode", "C:\\Windows\\Fonts\\l_10646.ttf"),
];

// ---------------------------------------------------------- status messages

/// Status bar text shown at startup.
pub const MSG_READY: &str = "就绪 · 输入 LaTeX 公式";
/// `{formats}` = clipboard formats that were actually written.
pub const MSG_COPY_OK: &str = "已写入 {formats}，可在 Word / PPT / WPS 中粘贴";
/// `{flavor}` = Markdown flavor name.
pub const MSG_MD_COPY_OK: &str = "已复制 Markdown（{flavor}）到剪贴板";
/// Import success text.
pub const MSG_IMPORT_OK: &str = "已从 Word 公式还原 LaTeX";
/// `{target}` / `{path}` / `{dpi}` / `{background}`.
pub const MSG_EXPORT_OK: &str = "已导出 {target}：{path}（DPI {dpi} / {background}）";
/// `{flavor}` / `{path}`.
pub const MSG_MD_EXPORT_OK: &str = "已导出 Markdown 片段（{flavor}）：{path}";
/// Shown when the user dismisses the native save dialog.
pub const MSG_CANCELLED: &str = "已取消导出";
/// MathML clipboard path unavailable.
pub const MSG_MATHML_UNAVAILABLE: &str = "MathML 路径不可用，已写入 OMML";
/// OMML unavailable, therefore docx export is unavailable.
pub const MSG_OMML_UNAVAILABLE: &str = "OMML 不可用：docx 导出已禁用，可导出 SVG/PNG/Markdown";
/// Extended formula: only the OMML clipboard path is trustworthy.
pub const MSG_EXTENDED: &str =
    "多行公式的剪贴板主路径不可用，已改用 OMML；如需完整结构建议导出 .docx";
/// Delimiter was not a balanced pair; the raw text is rendered as-is.
pub const MSG_DELIMITER_WARN: &str = "未识别到成对定界符，已按原文渲染";
/// Clipboard holds neither OMML nor a LaTeX-looking plain text.
pub const MSG_NO_FORMULA: &str = "剪贴板中没有可识别的公式，请在 Word 中复制公式对象";
/// Unsupported construct report; `{commands}` is filled by the app.
pub const MSG_UNSUPPORTED: &str = "不支持该构造：{commands}";
/// Clipboard could not be opened.
pub const MSG_CLIPBOARD_BUSY: &str = "剪贴板被其它程序占用，可改用文件导出";
/// Both text converters failed, only the plain LaTeX fallback was written.
pub const MSG_PLAIN_ONLY: &str = "仅复制了 LaTeX 源码；可用 SVG/PNG/Markdown 导出";
/// Empty source text for E103.
pub const MSG_EMPTY_SOURCE: &str = "源码为空，请输入 LaTeX 公式";
/// Source length exceeded [`MAX_SOURCE_BYTES`] for E101.
pub const MSG_TOO_LARGE: &str = "源码超过 64 KiB 上限，请拆分公式";
/// Brace nesting exceeded [`MAX_NESTING_DEPTH`] for E102.
pub const MSG_TOO_DEEP: &str = "公式嵌套过深，请简化结构";
/// docx export could not obtain OMML for E602.
pub const MSG_DOCX_OMML_FAILED: &str = "docx 所需 OMML 转换失败";
/// No system CJK font was readable, so UI labels may fall back to tofu boxes.
pub const MSG_UI_FONT_MISSING: &str = "未找到系统中文字体，界面文字可能显示为方块";
/// Configuration file could not be read or parsed.
pub const MSG_CONFIG_LOAD_FAILED: &str = "常用公式配置读取失败，已使用默认配置";
/// Configuration file could not be written.
pub const MSG_CONFIG_SAVE_FAILED: &str = "常用公式配置保存失败";
/// The executable directory is unavailable, so no configuration path exists.
pub const MSG_CONFIG_PATH_FAILED: &str = "无法定位程序所在目录，常用公式配置不可用";
/// `{path}` = configuration file path.
pub const MSG_CONFIG_SAVED: &str = "常用公式配置已保存：{path}";
/// Toolbar hover text: action name, effect and shortcut of the Office copy button.
pub const HINT_COPY_OFFICE: &str =
    "复制为 Office 公式（Ctrl+Shift+C）\n写入 MathML / OMML / HTML，Word / PPT / WPS 中粘贴得可编辑公式";
/// Toolbar hover text: action name, effect and shortcut of the Markdown copy button.
pub const HINT_COPY_MARKDOWN: &str =
    "复制为 Markdown（Ctrl+Shift+M）\n按「设置 → Markdown 形态」包装为纯文本";

// -------------------------------------------------------------------- types

/// Everything that makes a rendered preview stale.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct RenderKey {
    /// Hash of the normalized body only.
    pub src_hash: u64,
    /// Preview font size in points.
    pub font_size_pt: i64,
    /// Display (true) or text (false) style.
    pub display: bool,
    /// `f32::to_bits` of `pixels_per_point`.
    pub ppp_bits: u32,
    /// Foreground colour derived from the current theme.
    pub color: [u8; 3],
}

impl RenderKey {
    /// Derives a key from the current UI state.
    #[must_use]
    pub fn new(
        body: &str,
        font_size_pt: i64,
        display: bool,
        pixels_per_point: f32,
        color: [u8; 3],
    ) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        body.hash(&mut hasher);
        Self {
            src_hash: hasher.finish(),
            font_size_pt,
            display,
            ppp_bits: pixels_per_point.to_bits(),
            color,
        }
    }
}

/// Cached preview result; immutable once produced.
pub struct Rendered {
    /// Key the shapes were produced for.
    pub key: RenderKey,
    /// Shapes already positioned at the requested origin.
    pub shapes: Vec<eframe::egui::Shape>,
    /// Layout bounding rectangle of the expression.
    pub rect: eframe::egui::Rect,
}

/// Export-only style, deliberately independent from [`RenderKey`].
#[derive(Clone, Copy, Debug)]
pub struct ExportStyle {
    /// Export font size in points.
    pub font_size_pt: i64,
    /// Display (true) or text (false) style.
    pub display: bool,
    /// Glyph colour; exports always use [`EXPORT_COLOR`].
    pub color: [u8; 3],
    /// PNG resolution.
    pub dpi: i64,
    /// PNG canvas behind the glyphs.
    pub background: PngBackgroundChoice,
}

impl Default for ExportStyle {
    fn default() -> Self {
        Self {
            font_size_pt: DEFAULT_EXPORT_FONT_SIZE_PT,
            display: false,
            color: EXPORT_COLOR,
            dpi: DEFAULT_DPI,
            background: DEFAULT_BACKGROUND,
        }
    }
}

/// PNG background choice exposed by the UI.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PngBackgroundChoice {
    /// Fully transparent canvas.
    Transparent,
    /// Opaque white canvas.
    White,
}

/// File export targets; the export surface is fixed to these four.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExportTarget {
    /// Self-contained SVG 1.1 document.
    Svg,
    /// Raster PNG.
    Png,
    /// Minimal OPC package with an editable OMML formula.
    Docx,
    /// Single-formula Markdown fragment.
    Markdown,
}

/// Detected input form and requested output form of a formula.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Delimiter {
    /// Bare LaTeX without delimiters.
    None,
    /// `$...$`
    Inline,
    /// `$$...$$`, delimiters on their own lines.
    Display,
    /// `\(...\)`
    MacroInline,
    /// `\[...\]`
    MacroDisplay,
    /// GitHub `math` fence.
    Fenced,
}

impl Delimiter {
    /// Name used for the `{flavor}` placeholder and the UI drop-down.
    #[must_use]
    pub fn flavor_name(self) -> &'static str {
        match self {
            Self::None => "裸 LaTeX",
            Self::Inline => "$…$",
            Self::Display => "$$…$$",
            Self::MacroInline => "\\(…\\)",
            Self::MacroDisplay => "\\[…\\]",
            Self::Fenced => "GitHub math 围栏",
        }
    }

    /// Prefix/suffix pair of this flavor, taken from [`MD_DELIMITERS`].
    #[must_use]
    pub fn wrappers(self) -> (&'static str, &'static str) {
        MD_DELIMITERS
            .iter()
            .find(|(delimiter, _, _)| *delimiter == self)
            .map_or(("", ""), |(_, prefix, suffix)| (*prefix, *suffix))
    }
}

/// Editor text plus its normalized body, which is the single source of truth.
pub struct NormalizedSource {
    /// Original editor text; never rewritten by an output action.
    ///
    /// The app keeps the editor buffer separately, so only tests and the
    /// import path consume this copy of the raw text.
    #[allow(dead_code)]
    pub raw: String,
    /// Normalized formula body consumed by every output chain.
    pub body: String,
    /// Initial value of the display/text switch, when the form implies one.
    pub style_hint: Option<bool>,
    /// Detected input form.
    pub delimiter: Delimiter,
    /// Delimiter warning text, surfaced by the app as E104.
    pub warning: Option<&'static str>,
}

/// Capability class of a formula.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CoverageLevel {
    /// All four output chains are guaranteed.
    Core,
    /// Preview, SVG/PNG and Markdown are guaranteed; Office paths are best effort.
    Extended,
    /// Rejected before any output chain runs.
    Unsupported,
}

/// Result of the whitelist scan.
#[derive(Clone, Debug)]
pub struct Coverage {
    /// Capability class.
    pub level: CoverageLevel,
    /// Commands or environment names that pushed the formula out of Core.
    pub blocked: Vec<&'static str>,
}

/// Status bar severity.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    /// Neutral information.
    Info,
    /// Recoverable degradation.
    Warn,
    /// Failed action.
    Error,
}

/// Single status bar entry.
#[derive(Clone, Debug)]
pub struct Notice {
    /// Severity, which drives the text colour.
    pub level: Level,
    /// Rendered text.
    pub text: String,
    /// Stable error code shown next to the text.
    pub code: Option<&'static str>,
}

impl Notice {
    /// Informational notice without an error code.
    #[must_use]
    pub fn info(text: impl Into<String>) -> Self {
        Self {
            level: Level::Info,
            text: text.into(),
            code: None,
        }
    }

    /// Warning notice carrying a stable code.
    #[must_use]
    pub fn warn(code: &'static str, text: impl Into<String>) -> Self {
        Self {
            level: Level::Warn,
            text: text.into(),
            code: Some(code),
        }
    }

    /// Error notice carrying a stable code.
    #[must_use]
    pub fn error(code: &'static str, text: impl Into<String>) -> Self {
        Self {
            level: Level::Error,
            text: text.into(),
            code: Some(code),
        }
    }
}

/// UI theme preference; `System` is delegated to the window backend.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThemeChoice {
    /// Follow the Windows light/dark setting.
    System,
    /// Force light visuals.
    Light,
    /// Force dark visuals.
    Dark,
}

/// Unified error type shared by the preview, clipboard and export chains.
#[derive(Debug, thiserror::Error)]
#[error("{code}: {message}")]
pub struct AppError {
    /// Stable code from the error table (§9.2).
    pub code: &'static str,
    /// Human readable message shown in the status bar.
    pub message: String,
    /// Optional lower level cause, kept for diagnostics.
    #[source]
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl AppError {
    /// Creates an error without a lower level cause.
    #[must_use]
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            source: None,
        }
    }

    /// Creates an error that keeps the wrapped cause for diagnostics.
    #[must_use]
    pub fn caused(
        code: &'static str,
        message: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }
}

// ----------------------------------------------------------- pure functions

/// Rejects oversized or too deeply nested sources before any parse happens.
#[must_use]
pub fn validate_source(body: &str) -> Option<AppError> {
    if body.len() > MAX_SOURCE_BYTES {
        return Some(AppError::new("E101", MSG_TOO_LARGE));
    }
    let mut depth = 0usize;
    for character in body.chars() {
        match character {
            '{' => {
                depth += 1;
                if depth > MAX_NESTING_DEPTH {
                    return Some(AppError::new("E102", MSG_TOO_DEEP));
                }
            }
            '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    None
}

/// Strips at most one outermost delimiter pair; never rewrites the body.
#[must_use]
pub fn normalize_source(raw: &str) -> NormalizedSource {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return normalized(raw, String::new(), None, Delimiter::None, None);
    }
    if let Some(body) = strip_fence(trimmed) {
        return normalized(raw, body, Some(true), Delimiter::Fenced, None);
    }
    if let Some(body) = strip_pair(trimmed, "$$", "$$") {
        if has_unescaped_dollar(body) {
            return rejected(raw, trimmed);
        }
        return normalized(
            raw,
            body.trim().to_owned(),
            Some(true),
            Delimiter::Display,
            None,
        );
    }
    if let Some(body) = strip_pair(trimmed, "$", "$") {
        let spaced = body.chars().next().is_some_and(char::is_whitespace)
            || body.chars().next_back().is_some_and(char::is_whitespace);
        if spaced {
            return normalized(raw, trimmed.to_owned(), None, Delimiter::None, None);
        }
        if has_unescaped_dollar(body) {
            return rejected(raw, trimmed);
        }
        return normalized(raw, body.to_owned(), Some(false), Delimiter::Inline, None);
    }
    if let Some(body) = strip_pair(trimmed, "\\(", "\\)") {
        return normalized(
            raw,
            body.to_owned(),
            Some(false),
            Delimiter::MacroInline,
            None,
        );
    }
    if let Some(body) = strip_pair(trimmed, "\\[", "\\]") {
        return normalized(
            raw,
            body.to_owned(),
            Some(true),
            Delimiter::MacroDisplay,
            None,
        );
    }
    if is_single_sided(trimmed) {
        return rejected(raw, trimmed);
    }
    normalized(raw, trimmed.to_owned(), None, Delimiter::None, None)
}

/// Wraps a normalized body in the delimiters of `flavor`; inverse of [`normalize_source`].
#[must_use]
pub fn to_markdown(body: &str, flavor: Delimiter) -> String {
    let (prefix, suffix) = flavor.wrappers();
    format!("{prefix}{body}{suffix}")
}

fn normalized(
    raw: &str,
    body: String,
    style_hint: Option<bool>,
    delimiter: Delimiter,
    warning: Option<&'static str>,
) -> NormalizedSource {
    NormalizedSource {
        raw: raw.to_owned(),
        body,
        style_hint,
        delimiter,
        warning,
    }
}

fn rejected(raw: &str, trimmed: &str) -> NormalizedSource {
    normalized(
        raw,
        trimmed.to_owned(),
        None,
        Delimiter::None,
        Some(MSG_DELIMITER_WARN),
    )
}

fn strip_pair<'a>(text: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    let rest = text.strip_prefix(prefix)?;
    let inner = rest.strip_suffix(suffix)?;
    // `$x$` needs at least one character; `$$` must not be mistaken for `$…$`.
    if inner.is_empty() && (prefix == "$" || prefix == "$$") {
        return None;
    }
    Some(inner)
}

fn strip_fence(text: &str) -> Option<String> {
    let rest = text.strip_prefix(MD_FENCE_OPEN)?;
    let rest = rest.strip_suffix("```")?;
    let rest = rest.strip_suffix('\n')?;
    Some(rest.trim().to_owned())
}

/// Detects an unescaped `$`, used to spot a second formula block or currency text.
fn has_unescaped_dollar(text: &str) -> bool {
    let bytes = text.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b'$' {
            continue;
        }
        let mut backslashes = 0usize;
        let mut cursor = index;
        while cursor > 0 && bytes[cursor - 1] == b'\\' {
            backslashes += 1;
            cursor -= 1;
        }
        if backslashes.is_multiple_of(2) {
            return true;
        }
    }
    false
}

/// True when the text looks like one half of a delimiter pair, ignoring `$10` prose.
fn is_single_sided(text: &str) -> bool {
    let starts_dollar = text.starts_with('$');
    let ends_dollar = text.ends_with('$');
    if starts_dollar != ends_dollar {
        let currency = text.as_bytes().get(1).is_some_and(u8::is_ascii_digit);
        return !(starts_dollar && currency);
    }
    [("\\(", "\\)"), ("\\[", "\\]")]
        .iter()
        .any(|(open, close)| text.starts_with(open) != text.ends_with(close))
}
