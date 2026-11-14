//! Persisted user configuration stored next to the executable.
//!
//! The schema is intentionally tiny (`version`, `favorites`, an optional
//! `window` rectangle and the settings-menu choices), so this module uses a
//! small, strict JSON reader and writer built on the standard library instead of
//! pulling a serialization framework into the binary. The file is written
//! atomically on every change so a crash can never leave a half-written document
//! behind.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::catalog;
use crate::model::{
    AppError, Delimiter, ExitSavePolicy, PngBackgroundChoice, ThemeChoice, CONFIG_FILE_NAME,
    CONFIG_VERSION, DEFAULT_BACKGROUND, DEFAULT_DPI, DEFAULT_FONT_SIZE_PT, DPI_CHOICES,
    FONT_SIZE_RANGE, MIN_WINDOW_SIZE, MSG_CONFIG_LOAD_FAILED, MSG_CONFIG_PATH_FAILED,
    MSG_CONFIG_SAVE_FAILED,
};

/// Upper bound (in points) accepted for a restored window size or coordinate.
///
/// Values beyond the range of a 16-bit screen coordinate are treated as corrupt
/// instead of being passed to the window backend.
const MAX_WINDOW_RECT_VALUE: f32 = 32_767.0;

/// Last known window rectangle, restored on the next start.
///
/// `position` is the outer top-left corner (frame included) and `size` is the
/// inner client size, both in egui points so they survive DPI changes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowGeometry {
    /// Outer top-left corner in screen points.
    pub position: [f32; 2],
    /// Inner client size in points.
    pub size: [f32; 2],
    /// Whether the window was maximized when it was closed.
    pub maximized: bool,
}

/// Persisted configuration. Unknown fields and invalid identifiers are rejected.
#[derive(Debug)]
pub struct Config {
    /// Schema version written by the current build.
    pub version: u32,
    /// Ordered identifiers of the formulas shown on the favorites toolbar.
    pub favorites: Vec<String>,
    /// Window rectangle captured at shutdown; `None` uses the default layout.
    pub window: Option<WindowGeometry>,
    /// Preview font size in points.
    pub font_size_pt: i64,
    /// Explicit Display (true) / Text (false) choice; `None` follows the input.
    pub display: Option<bool>,
    /// UI theme preference.
    pub theme: ThemeChoice,
    /// PNG export background.
    pub png_background: PngBackgroundChoice,
    /// PNG export resolution.
    pub dpi: i64,
    /// Markdown output flavor; `None` follows the input delimiters.
    pub markdown_flavor: Option<Delimiter>,
    /// Criterion deciding whether closing the window needs confirmation.
    pub exit_policy: ExitSavePolicy,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            favorites: catalog::default_favorites(),
            window: None,
            font_size_pt: DEFAULT_FONT_SIZE_PT,
            display: None,
            theme: ThemeChoice::System,
            png_background: DEFAULT_BACKGROUND,
            dpi: DEFAULT_DPI,
            markdown_flavor: None,
            exit_policy: ExitSavePolicy::default(),
        }
    }
}

/// Reads the configuration next to the executable; a missing file yields defaults.
pub fn load() -> Result<Config, AppError> {
    load_from(&config_path()?)
}

/// Writes the configuration next to the executable and returns the file path.
pub fn save(config: &Config) -> Result<PathBuf, AppError> {
    let path = config_path()?;
    save_to(&path, config)?;
    Ok(path)
}

fn config_path() -> Result<PathBuf, AppError> {
    let executable = std::env::current_exe()
        .map_err(|error| AppError::caused("E701", MSG_CONFIG_PATH_FAILED, error))?;
    let directory = executable
        .parent()
        .ok_or_else(|| AppError::new("E701", MSG_CONFIG_PATH_FAILED))?;
    Ok(directory.join(CONFIG_FILE_NAME))
}

fn load_from(path: &Path) -> Result<Config, AppError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Config::default()),
        Err(error) => {
            return Err(AppError::caused(
                "E701",
                format!("{MSG_CONFIG_LOAD_FAILED}：{}", path.display()),
                error,
            ))
        }
    };
    parse_json(&text).map(Config::sanitized).map_err(|detail| {
        AppError::new(
            "E701",
            format!("{MSG_CONFIG_LOAD_FAILED}：{}（{detail}）", path.display()),
        )
    })
}

fn save_to(path: &Path, config: &Config) -> Result<(), AppError> {
    let config = Config {
        version: CONFIG_VERSION,
        favorites: config.favorites.clone(),
        window: config.window,
        font_size_pt: config.font_size_pt,
        display: config.display,
        theme: config.theme,
        png_background: config.png_background,
        dpi: config.dpi,
        markdown_flavor: config.markdown_flavor,
        exit_policy: config.exit_policy,
    }
    .sanitized();
    write_atomically(path, to_json(&config).as_bytes())
}

impl Config {
    /// Drops unknown identifiers, clamps out-of-range choices and falls back to defaults.
    fn sanitized(mut self) -> Self {
        let had_entries = !self.favorites.is_empty();
        self.favorites = catalog::sanitize(&self.favorites);
        if had_entries && self.favorites.is_empty() {
            self.favorites = catalog::default_favorites();
        }
        self.version = CONFIG_VERSION;
        self.window = self.window.and_then(WindowGeometry::sanitized);
        self.font_size_pt = self
            .font_size_pt
            .clamp(*FONT_SIZE_RANGE.start(), *FONT_SIZE_RANGE.end());
        if !DPI_CHOICES.contains(&self.dpi) {
            self.dpi = DEFAULT_DPI;
        }
        self
    }
}

impl WindowGeometry {
    /// Rejects non-finite or out-of-range values and clamps the size.
    fn sanitized(self) -> Option<Self> {
        let in_range = |value: f32| value.is_finite() && value.abs() <= MAX_WINDOW_RECT_VALUE;
        if !self.position.iter().copied().all(in_range) {
            return None;
        }
        if !self.size.iter().copied().all(f32::is_finite) {
            return None;
        }
        Some(Self {
            size: [
                self.size[0].clamp(MIN_WINDOW_SIZE[0], MAX_WINDOW_RECT_VALUE),
                self.size[1].clamp(MIN_WINDOW_SIZE[1], MAX_WINDOW_RECT_VALUE),
            ],
            ..self
        })
    }
}

/// Writes to a sibling temporary file, then renames over the destination.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(CONFIG_FILE_NAME);
    let temporary = path.with_file_name(format!(".{file_name}.{}.{stamp}.tmp", std::process::id()));
    fs::write(&temporary, bytes).map_err(|error| save_error(path, error))?;
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(save_error(path, error));
    }
    Ok(())
}

fn save_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::caused(
        "E701",
        format!("{MSG_CONFIG_SAVE_FAILED}：{}", path.display()),
        error,
    )
}

fn to_json(config: &Config) -> String {
    let mut members = vec![
        format!("  \"version\": {}", config.version),
        format!("  \"favorites\": [{}]", quoted_list(&config.favorites)),
    ];
    if let Some(window) = config.window {
        members.push(format!(
            "  \"window\": {{\"x\": {}, \"y\": {}, \"width\": {}, \"height\": {}, \"maximized\": {}}}",
            f32_literal(window.position[0]),
            f32_literal(window.position[1]),
            f32_literal(window.size[0]),
            f32_literal(window.size[1]),
            window.maximized
        ));
    }
    members.push(format!("  \"font_size_pt\": {}", config.font_size_pt));
    members.push(format!(
        "  \"display\": {}",
        optional_bool_literal(config.display)
    ));
    members.push(format!("  \"theme\": \"{}\"", theme_wire(config.theme)));
    members.push(format!(
        "  \"png_background\": \"{}\"",
        background_wire(config.png_background)
    ));
    members.push(format!("  \"dpi\": {}", config.dpi));
    members.push(format!(
        "  \"markdown_flavor\": {}",
        match config.markdown_flavor.and_then(flavor_wire) {
            Some(flavor) => format!("\"{flavor}\""),
            None => "null".to_owned(),
        }
    ));
    members.push(format!(
        "  \"exit_policy\": \"{}\"",
        exit_policy_wire(config.exit_policy)
    ));
    format!("{{\n{}\n}}\n", members.join(",\n"))
}

/// Quotes a list of identifiers in one line, escaping each value.
fn quoted_list(values: &[String]) -> String {
    values
        .iter()
        .map(|value| {
            let mut quoted = String::with_capacity(value.len() + 2);
            quoted.push('"');
            push_escaped(&mut quoted, value);
            quoted.push('"');
            quoted
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Appends an `f32` without a trailing `.0`, so the document stays readable.
fn f32_literal(value: f32) -> String {
    format!("{value}")
}

fn optional_bool_literal(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "true",
        Some(false) => "false",
        None => "null",
    }
}

fn theme_wire(theme: ThemeChoice) -> &'static str {
    match theme {
        ThemeChoice::System => "system",
        ThemeChoice::Light => "light",
        ThemeChoice::Dark => "dark",
    }
}

fn theme_from_wire(value: &str) -> Option<ThemeChoice> {
    match value {
        "system" => Some(ThemeChoice::System),
        "light" => Some(ThemeChoice::Light),
        "dark" => Some(ThemeChoice::Dark),
        _ => None,
    }
}

fn background_wire(background: PngBackgroundChoice) -> &'static str {
    match background {
        PngBackgroundChoice::Transparent => "transparent",
        PngBackgroundChoice::White => "white",
    }
}

fn background_from_wire(value: &str) -> Option<PngBackgroundChoice> {
    match value {
        "transparent" => Some(PngBackgroundChoice::Transparent),
        "white" => Some(PngBackgroundChoice::White),
        _ => None,
    }
}

/// Maps a selectable flavor to its JSON literal; the internal pass-through
/// form [`Delimiter::None`] is not selectable and serializes as `null`.
fn flavor_wire(flavor: Delimiter) -> Option<&'static str> {
    match flavor {
        Delimiter::Inline => Some("inline"),
        Delimiter::Display => Some("display"),
        Delimiter::Fenced => Some("fenced"),
        Delimiter::None => None,
    }
}

fn flavor_from_wire(value: &str) -> Option<Delimiter> {
    match value {
        "inline" => Some(Delimiter::Inline),
        "display" => Some(Delimiter::Display),
        "fenced" => Some(Delimiter::Fenced),
        _ => None,
    }
}

fn exit_policy_wire(policy: ExitSavePolicy) -> &'static str {
    match policy {
        ExitSavePolicy::FileExportOnly => "file_export_only",
        ExitSavePolicy::AnyOutput => "any_output",
    }
}

fn exit_policy_from_wire(value: &str) -> Option<ExitSavePolicy> {
    match value {
        "file_export_only" => Some(ExitSavePolicy::FileExportOnly),
        "any_output" => Some(ExitSavePolicy::AnyOutput),
        _ => None,
    }
}

fn push_escaped(output: &mut String, value: &str) {
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            control if control < ' ' => {
                output.push_str(&format!("\\u{:04x}", u32::from(control)));
            }
            other => output.push(other),
        }
    }
}

/// Parses the configuration object.
///
/// The reader is strict about structure (no trailing commas, unknown fields, or
/// trailing content) because every failure falls back to defaults with an E701
/// notice; silently accepting a damaged file would hide the problem.
fn parse_json(text: &str) -> Result<Config, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut cursor = JsonCursor::new(text);
    cursor.skip_whitespace();
    cursor.expect('{')?;
    cursor.skip_whitespace();
    if cursor.peek() == Some('}') {
        cursor.bump();
        return cursor.finish(Config::default());
    }
    let mut version = CONFIG_VERSION;
    let mut favorites = None;
    let mut window = None;
    let mut font_size_pt = DEFAULT_FONT_SIZE_PT;
    let mut display = None;
    let mut theme = ThemeChoice::System;
    let mut png_background = DEFAULT_BACKGROUND;
    let mut dpi = DEFAULT_DPI;
    let mut markdown_flavor = None;
    let mut exit_policy = ExitSavePolicy::default();
    loop {
        let key = cursor.parse_string()?;
        cursor.expect(':')?;
        match key.as_str() {
            "version" => version = cursor.parse_u32()?,
            "favorites" => favorites = Some(cursor.parse_string_array()?),
            "window" => window = cursor.parse_optional_window()?,
            "font_size_pt" => font_size_pt = i64::from(cursor.parse_u32()?),
            "display" => display = cursor.parse_optional_bool()?,
            "theme" => {
                let value = cursor.parse_string()?;
                theme = theme_from_wire(&value)
                    .ok_or_else(|| cursor.error(&format!("未知主题 \"{value}\"")))?;
            }
            "png_background" => {
                let value = cursor.parse_string()?;
                png_background = background_from_wire(&value)
                    .ok_or_else(|| cursor.error(&format!("未知 PNG 背景 \"{value}\"")))?;
            }
            "dpi" => dpi = i64::from(cursor.parse_u32()?),
            "markdown_flavor" => {
                markdown_flavor =
                    match cursor.parse_optional_string()?.as_deref() {
                        None => None,
                        Some(value) => Some(flavor_from_wire(value).ok_or_else(|| {
                            cursor.error(&format!("未知 Markdown 形态 \"{value}\""))
                        })?),
                    };
            }
            "exit_policy" => {
                let value = cursor.parse_string()?;
                exit_policy = exit_policy_from_wire(&value)
                    .ok_or_else(|| cursor.error(&format!("未知退出提示策略 \"{value}\"")))?;
            }
            other => return Err(cursor.error(&format!("未知字段 \"{other}\""))),
        }
        cursor.skip_whitespace();
        match cursor.bump() {
            Some(',') => cursor.skip_whitespace(),
            Some('}') => break,
            Some(other) => {
                return Err(cursor.error(&format!("对象成员之间期望 ',' 或 '}}'，实际 '{other}'")))
            }
            None => return Err(cursor.error("对象未闭合")),
        }
    }
    let config = Config {
        version,
        favorites: favorites.unwrap_or_else(catalog::default_favorites),
        window,
        font_size_pt,
        display,
        theme,
        png_background,
        dpi,
        markdown_flavor,
        exit_policy,
    };
    cursor.finish(config)
}

struct JsonCursor<'a> {
    text: &'a str,
    position: usize,
}

impl<'a> JsonCursor<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, position: 0 }
    }

    fn peek(&self) -> Option<char> {
        self.text[self.position..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let character = self.peek()?;
        self.position += character.len_utf8();
        Some(character)
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(character) if character.is_whitespace()) {
            self.bump();
        }
    }

    fn error(&self, message: &str) -> String {
        format!("第 {} 字节：{message}", self.position)
    }

    fn expect(&mut self, expected: char) -> Result<(), String> {
        self.skip_whitespace();
        match self.bump() {
            Some(actual) if actual == expected => Ok(()),
            Some(actual) => Err(self.error(&format!("期望 '{expected}'，实际 '{actual}'"))),
            None => Err(self.error(&format!("期望 '{expected}'，但已到末尾"))),
        }
    }

    fn finish(mut self, config: Config) -> Result<Config, String> {
        self.skip_whitespace();
        if self.peek().is_some() {
            return Err(self.error("对象之后存在多余内容"));
        }
        Ok(config)
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.skip_whitespace();
        if self.bump() != Some('"') {
            return Err(self.error("期望字符串"));
        }
        let mut value = String::new();
        loop {
            match self.bump() {
                None => return Err(self.error("字符串未闭合")),
                Some('"') => return Ok(value),
                Some('\\') => value.push(self.parse_escape()?),
                Some(control) if control < ' ' => {
                    return Err(self.error("字符串含未转义的控制字符"))
                }
                Some(character) => value.push(character),
            }
        }
    }

    fn parse_escape(&mut self) -> Result<char, String> {
        match self.bump() {
            Some('"') => Ok('"'),
            Some('\\') => Ok('\\'),
            Some('/') => Ok('/'),
            Some('b') => Ok('\u{0008}'),
            Some('f') => Ok('\u{000C}'),
            Some('n') => Ok('\n'),
            Some('r') => Ok('\r'),
            Some('t') => Ok('\t'),
            Some('u') => {
                let code = self.parse_hex4()?;
                char::from_u32(code).ok_or_else(|| self.error("不支持的 Unicode 转义"))
            }
            Some(other) => Err(self.error(&format!("非法转义 \\{other}"))),
            None => Err(self.error("转义序列未完成")),
        }
    }

    fn parse_hex4(&mut self) -> Result<u32, String> {
        let mut code = 0_u32;
        for _ in 0..4 {
            let digit = self
                .bump()
                .and_then(|character| character.to_digit(16))
                .ok_or_else(|| self.error("Unicode 转义需要 4 位十六进制数"))?;
            code = code * 16 + digit;
        }
        Ok(code)
    }

    fn parse_u32(&mut self) -> Result<u32, String> {
        self.skip_whitespace();
        let start = self.position;
        while matches!(self.peek(), Some(character) if character.is_ascii_digit()) {
            self.bump();
        }
        if self.position == start {
            return Err(self.error("期望整数"));
        }
        self.text[start..self.position]
            .parse::<u32>()
            .map_err(|_| self.error("整数超出范围"))
    }

    fn parse_f32(&mut self) -> Result<f32, String> {
        self.skip_whitespace();
        let start = self.position;
        while matches!(
            self.peek(),
            Some(character)
                if character.is_ascii_digit() || matches!(character, '-' | '+' | '.' | 'e' | 'E')
        ) {
            self.bump();
        }
        if self.position == start {
            return Err(self.error("期望数字"));
        }
        self.text[start..self.position]
            .parse::<f32>()
            .map_err(|_| self.error("数字格式非法"))
    }

    fn parse_bool(&mut self) -> Result<bool, String> {
        self.skip_whitespace();
        for (literal, value) in [("true", true), ("false", false)] {
            if self.text[self.position..].starts_with(literal) {
                self.position += literal.len();
                return Ok(value);
            }
        }
        Err(self.error("期望 true 或 false"))
    }

    /// Parses an optional boolean member; `null` explicitly clears the choice.
    fn parse_optional_bool(&mut self) -> Result<Option<bool>, String> {
        self.skip_whitespace();
        if self.consume_null() {
            return Ok(None);
        }
        self.parse_bool().map(Some)
    }

    /// Parses an optional string member; `null` explicitly clears the choice.
    fn parse_optional_string(&mut self) -> Result<Option<String>, String> {
        self.skip_whitespace();
        if self.consume_null() {
            return Ok(None);
        }
        self.parse_string().map(Some)
    }

    /// Consumes the `null` literal when present and reports whether it matched.
    fn consume_null(&mut self) -> bool {
        if self.text[self.position..].starts_with("null") {
            self.position += "null".len();
            return true;
        }
        false
    }

    /// Parses the optional `window` member; `null` explicitly disables it.
    fn parse_optional_window(&mut self) -> Result<Option<WindowGeometry>, String> {
        self.skip_whitespace();
        if self.consume_null() {
            return Ok(None);
        }
        self.expect('{')?;
        let mut position = [None; 2];
        let mut size = [None; 2];
        let mut maximized = false;
        self.skip_whitespace();
        if self.peek() == Some('}') {
            self.bump();
        } else {
            loop {
                let key = self.parse_string()?;
                self.expect(':')?;
                match key.as_str() {
                    "x" => position[0] = Some(self.parse_f32()?),
                    "y" => position[1] = Some(self.parse_f32()?),
                    "width" => size[0] = Some(self.parse_f32()?),
                    "height" => size[1] = Some(self.parse_f32()?),
                    "maximized" => maximized = self.parse_bool()?,
                    other => return Err(self.error(&format!("未知字段 \"{other}\""))),
                }
                self.skip_whitespace();
                match self.bump() {
                    Some(',') => self.skip_whitespace(),
                    Some('}') => break,
                    Some(other) => {
                        return Err(
                            self.error(&format!("对象成员之间期望 ',' 或 '}}'，实际 '{other}'"))
                        )
                    }
                    None => return Err(self.error("window 对象未闭合")),
                }
            }
        }
        let (Some(x), Some(y), Some(width), Some(height)) =
            (position[0], position[1], size[0], size[1])
        else {
            return Err(self.error("window 缺少 x / y / width / height"));
        };
        Ok(Some(WindowGeometry {
            position: [x, y],
            size: [width, height],
            maximized,
        }))
    }

    fn parse_string_array(&mut self) -> Result<Vec<String>, String> {
        self.expect('[')?;
        let mut values = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(']') {
            self.bump();
            return Ok(values);
        }
        loop {
            values.push(self.parse_string()?);
            self.skip_whitespace();
            match self.bump() {
                Some(',') => self.skip_whitespace(),
                Some(']') => return Ok(values),
                Some(other) => {
                    return Err(self.error(&format!("数组元素之间期望 ',' 或 ']'，实际 '{other}'")))
                }
                None => return Err(self.error("数组未闭合")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "r-mathtex-config-{}-{name}.json",
            std::process::id()
        ))
    }

    #[test]
    fn missing_file_yields_defaults() {
        let path = temporary_path("missing");
        let _ = fs::remove_file(&path);
        let config = load_from(&path).expect("missing file must use defaults");
        assert_eq!(config.favorites, catalog::default_favorites());
    }

    #[test]
    fn roundtrip_preserves_favorites() {
        let path = temporary_path("roundtrip");
        let config = Config {
            favorites: vec!["sqrt".to_owned(), "frac".to_owned()],
            ..Config::default()
        };
        save_to(&path, &config).expect("save must succeed");
        let config = load_from(&path).expect("load must succeed");
        assert_eq!(config.favorites, vec!["frac".to_owned(), "sqrt".to_owned()]);
        assert_eq!(config.window, None);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn roundtrip_preserves_window_geometry() {
        let path = temporary_path("window");
        let window = WindowGeometry {
            position: [-120.5, 64.0],
            size: [1280.0, 800.0],
            maximized: true,
        };
        let config = Config {
            window: Some(window),
            ..Config::default()
        };
        save_to(&path, &config).expect("save must succeed");
        let config = load_from(&path).expect("load must succeed");
        assert_eq!(config.window, Some(window));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn roundtrip_preserves_settings() {
        let path = temporary_path("settings");
        let config = Config {
            font_size_pt: 22,
            display: Some(true),
            theme: ThemeChoice::Dark,
            png_background: PngBackgroundChoice::White,
            dpi: 600,
            markdown_flavor: Some(Delimiter::Fenced),
            exit_policy: ExitSavePolicy::AnyOutput,
            ..Config::default()
        };
        save_to(&path, &config).expect("save must succeed");
        let loaded = load_from(&path).expect("load must succeed");
        assert_eq!(loaded.font_size_pt, 22);
        assert_eq!(loaded.display, Some(true));
        assert_eq!(loaded.theme, ThemeChoice::Dark);
        assert_eq!(loaded.png_background, PngBackgroundChoice::White);
        assert_eq!(loaded.dpi, 600);
        assert_eq!(loaded.markdown_flavor, Some(Delimiter::Fenced));
        assert_eq!(loaded.exit_policy, ExitSavePolicy::AnyOutput);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn missing_settings_use_defaults() {
        let config = parse_json(r#"{"version":1}"#).expect("partial config must parse");
        let defaults = Config::default();
        assert_eq!(config.font_size_pt, defaults.font_size_pt);
        assert_eq!(config.display, defaults.display);
        assert_eq!(config.theme, defaults.theme);
        assert_eq!(config.png_background, defaults.png_background);
        assert_eq!(config.dpi, defaults.dpi);
        assert_eq!(config.markdown_flavor, defaults.markdown_flavor);
        assert_eq!(config.exit_policy, defaults.exit_policy);
    }

    #[test]
    fn out_of_range_settings_fall_back() {
        let config = parse_json(r#"{"font_size_pt":99,"dpi":123}"#)
            .expect("numeric settings must parse")
            .sanitized();
        assert_eq!(config.font_size_pt, *FONT_SIZE_RANGE.end());
        assert_eq!(config.dpi, DEFAULT_DPI);
    }

    #[test]
    fn unknown_setting_values_are_rejected() {
        assert!(parse_json(r#"{"theme":"neon"}"#).is_err());
        assert!(parse_json(r#"{"png_background":"blue"}"#).is_err());
        assert!(parse_json(r#"{"markdown_flavor":"none"}"#).is_err());
        assert!(parse_json(r#"{"exit_policy":"sometimes"}"#).is_err());
    }

    #[test]
    fn window_null_disables_geometry() {
        let text = r#"{"favorites":["frac"],"window":null}"#;
        let config = parse_json(text).expect("null window must parse");
        assert_eq!(config.window, None);
    }

    #[test]
    fn invalid_window_geometry_is_dropped_or_rejected() {
        let clamped = parse_json(r#"{"window":{"x":0,"y":0,"width":10,"height":10}}"#)
            .expect("complete window must parse")
            .sanitized();
        assert_eq!(
            clamped.window.map(|window| window.size),
            Some(MIN_WINDOW_SIZE)
        );
        assert!(parse_json(r#"{"window":{"x":0,"y":0,"width":10}}"#).is_err());
        let dropped = parse_json(r#"{"window":{"x":9e30,"y":0,"width":10,"height":10}}"#)
            .expect("finite out-of-range window must parse")
            .sanitized();
        assert_eq!(dropped.window, None);
    }

    #[test]
    fn malformed_file_is_reported() {
        let path = temporary_path("malformed");
        fs::write(&path, "{not json").expect("fixture must be writable");
        assert!(load_from(&path).is_err());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn accepts_bom_whitespace_and_unknown_ids() {
        let text = "\u{feff}\n{ \"version\": 7, \"favorites\": [\"sqrt\", \"unknown\"] }\n";
        let config = parse_json(text)
            .expect("valid subset must parse")
            .sanitized();
        assert_eq!(config.version, CONFIG_VERSION);
        assert_eq!(config.favorites, vec!["sqrt".to_owned()]);
    }

    #[test]
    fn rejects_unknown_fields_and_trailing_content() {
        assert!(parse_json(r#"{"version":1,"extra":true}"#).is_err());
        assert!(parse_json(r#"{"version":1} trailing"#).is_err());
        assert!(parse_json(r#"{"version":1,}"#).is_err());
    }
}
