//! Persisted user configuration stored next to the executable.
//!
//! The schema is intentionally tiny (`version`, `favorites` and an optional
//! `window` rectangle), so this module uses a small, strict JSON reader and
//! writer built on the standard library instead of pulling a serialization
//! framework into the binary. The file is written atomically on every change so
//! a crash can never leave a half-written document behind.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::catalog;
use crate::model::{
    AppError, CONFIG_FILE_NAME, CONFIG_VERSION, MIN_WINDOW_SIZE, MSG_CONFIG_LOAD_FAILED,
    MSG_CONFIG_PATH_FAILED, MSG_CONFIG_SAVE_FAILED,
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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            favorites: catalog::default_favorites(),
            window: None,
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
    }
    .sanitized();
    write_atomically(path, to_json(&config).as_bytes())
}

impl Config {
    /// Drops unknown identifiers, corrupt geometry and falls back to defaults.
    fn sanitized(mut self) -> Self {
        let had_entries = !self.favorites.is_empty();
        self.favorites = catalog::sanitize(&self.favorites);
        if had_entries && self.favorites.is_empty() {
            self.favorites = catalog::default_favorites();
        }
        self.version = CONFIG_VERSION;
        self.window = self.window.and_then(WindowGeometry::sanitized);
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
    let mut json = String::from("{\n  \"version\": ");
    json.push_str(&config.version.to_string());
    json.push_str(",\n  \"favorites\": [");
    for (index, id) in config.favorites.iter().enumerate() {
        if index > 0 {
            json.push_str(", ");
        }
        json.push('"');
        push_escaped(&mut json, id);
        json.push('"');
    }
    json.push(']');
    if let Some(window) = config.window {
        json.push_str(",\n  \"window\": {\"x\": ");
        push_f32(&mut json, window.position[0]);
        json.push_str(", \"y\": ");
        push_f32(&mut json, window.position[1]);
        json.push_str(", \"width\": ");
        push_f32(&mut json, window.size[0]);
        json.push_str(", \"height\": ");
        push_f32(&mut json, window.size[1]);
        json.push_str(", \"maximized\": ");
        json.push_str(if window.maximized { "true" } else { "false" });
        json.push('}');
    }
    json.push_str("\n}\n");
    json
}

/// Appends an `f32` without a trailing `.0`, so the document stays readable.
fn push_f32(output: &mut String, value: f32) {
    output.push_str(&format!("{value}"));
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
    loop {
        let key = cursor.parse_string()?;
        cursor.expect(':')?;
        match key.as_str() {
            "version" => version = cursor.parse_u32()?,
            "favorites" => favorites = Some(cursor.parse_string_array()?),
            "window" => window = cursor.parse_optional_window()?,
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

    /// Parses the optional `window` member; `null` explicitly disables it.
    fn parse_optional_window(&mut self) -> Result<Option<WindowGeometry>, String> {
        self.skip_whitespace();
        if self.text[self.position..].starts_with("null") {
            self.position += "null".len();
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
