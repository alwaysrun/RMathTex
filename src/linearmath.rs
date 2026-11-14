//! Recovery of Word's UnicodeMath linear text into LaTeX.
//!
//! Word puts an equation's linear (UnicodeMath) form on the clipboard as
//! `CF_UNICODETEXT`, and that form is not LaTeX: equation arrays start with
//! `█` (`\eqarray`), matrices with `■` (`\matrix`), rows are separated by `@`,
//! alignment points by `&`, and symbols stay as literal Unicode characters.
//! This module reverses that encoding for every construct the editor renders.

/// Word's linear marker for an equation array (`\eqarray`, U+2588).
const EQ_ARRAY_MARKER: char = '█';
/// Word's linear marker for a matrix (`\matrix`, U+25A0).
const MATRIX_MARKER: char = '■';
/// Row separator inside a linear array.
const ROW_SEPARATOR: char = '@';
/// Alignment point inside a linear array.
const CELL_SEPARATOR: char = '&';
/// No-break space, which Word uses as an ordinary space in linear text.
const NO_BREAK_SPACE: char = '\u{00A0}';
/// Zero-width space Word may leave between runs.
const ZERO_WIDTH_SPACE: char = '\u{200B}';
/// Invisible function-application operator (U+2061).
const INVISIBLE_FUNCTION: char = '\u{2061}';
/// Invisible times operator (U+2062).
const INVISIBLE_TIMES: char = '\u{2062}';
/// Invisible separator (U+2063).
const INVISIBLE_SEPARATOR: char = '\u{2063}';

/// Whether `text` carries a Word equation array.
///
/// Word's OMML export drops the `&` alignment points of an equation array but
/// its linear text keeps them, so callers should prefer [`to_latex`] for this
/// shape.
#[must_use]
pub fn has_equation_array(text: &str) -> bool {
    text.contains(EQ_ARRAY_MARKER) || text.contains("\\eqarray")
}

/// Converts a Word linear-format formula into LaTeX.
///
/// Returns `None` when the text carries no recognizable linear math, so the
/// caller can leave ordinary clipboard text (including existing LaTeX) alone.
#[must_use]
pub fn to_latex(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let converted = convert_segment(trimmed);
    if !converted.recognized {
        return None;
    }
    let latex = converted.latex.trim();
    (!latex.is_empty()).then(|| latex.to_owned())
}

/// One conversion result plus whether any linear construct was rewritten.
struct Converted {
    latex: String,
    recognized: bool,
}

fn convert_segment(text: &str) -> Converted {
    let mut latex = String::with_capacity(text.len());
    let mut recognized = false;
    let mut index = 0usize;
    while index < text.len() {
        let character = text[index..]
            .chars()
            .next()
            .expect("index always lands on a char boundary");
        let handled = match character {
            EQ_ARRAY_MARKER | MATRIX_MARKER => parse_array(&text[index..]).map(|(block, used)| {
                latex.push_str(&block);
                recognized = true;
                index + used
            }),
            '√' => parse_radical(&text[index..]).map(|(radical, used)| {
                latex.push_str(&radical);
                recognized = true;
                index + used
            }),
            '\\' => convert_command(text, index).map(|(command, next, converted)| {
                latex.push_str(&command);
                recognized |= converted;
                next
            }),
            '_' | '^' => parse_script(&text[index..]).map(|(script, used)| {
                latex.push_str(&script);
                recognized = true;
                index + used
            }),
            '"' => parse_quoted(&text[index..]).map(|(quoted, used)| {
                latex.push_str(&quoted);
                recognized = true;
                index + used
            }),
            NO_BREAK_SPACE => {
                latex.push(' ');
                Some(index + character.len_utf8())
            }
            ZERO_WIDTH_SPACE | INVISIBLE_FUNCTION | INVISIBLE_TIMES | INVISIBLE_SEPARATOR => {
                Some(index + character.len_utf8())
            }
            _ => None,
        };
        index = match handled {
            Some(next) => next,
            None => {
                match symbol_latex(character) {
                    Some(command) => {
                        latex.push_str(command);
                        recognized = true;
                    }
                    None => latex.push(character),
                }
                index + character.len_utf8()
            }
        };
    }
    Converted { latex, recognized }
}

/// Parses `█(...)` / `■(...)`, the built-up form of an equation array or matrix.
fn parse_array(text: &str) -> Option<(String, usize)> {
    let marker = text.chars().next()?;
    if marker != EQ_ARRAY_MARKER && marker != MATRIX_MARKER {
        return None;
    }
    let rest = &text[marker.len_utf8()..];
    let opener = rest.chars().next()?;
    let closer = matching_closer(opener)?;
    let (inner, used) = read_group(rest, opener, closer)?;
    let latex = if marker == EQ_ARRAY_MARKER {
        array_from_rows(inner, ArrayKind::EquationArray)
    } else {
        let rows = split_rows(inner);
        let cells = rows
            .iter()
            .map(|row| split_top_level(row, CELL_SEPARATOR))
            .collect::<Vec<_>>();
        array_latex(matrix_environment(opener), &cells)
    };
    Some((latex, marker.len_utf8() + used))
}

/// Parses `√(x)` and `√[n](x)`.
fn parse_radical(text: &str) -> Option<(String, usize)> {
    let marker = text.chars().next()?;
    if marker != '√' {
        return None;
    }
    let rest = &text[marker.len_utf8()..];
    let (degree, body_start) = match read_group(rest, '[', ']') {
        Some((degree, used)) => (Some(convert_segment(degree).latex), used),
        None => (None, 0),
    };
    let (body, used) = read_group(&rest[body_start..], '(', ')')
        .or_else(|| read_group(&rest[body_start..], '{', '}'))?;
    let body = convert_segment(body).latex;
    let latex = match degree {
        Some(degree) => format!("\\sqrt[{degree}]{{{body}}}"),
        None => format!("\\sqrt{{{body}}}"),
    };
    Some((latex, marker.len_utf8() + body_start + used))
}

/// Parses `_(...)` / `^(...)`, the linear spelling of a braced script group.
fn parse_script(text: &str) -> Option<(String, usize)> {
    let marker = text.chars().next()?;
    if marker != '_' && marker != '^' {
        return None;
    }
    let rest = &text[marker.len_utf8()..];
    let (body, used) = read_group(rest, '(', ')')?;
    let body = convert_segment(body).latex;
    Some((format!("{marker}{{{body}}}"), marker.len_utf8() + used))
}

/// Parses a quoted text run, which Word stores as `"text"`.
fn parse_quoted(text: &str) -> Option<(String, usize)> {
    let rest = text.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some((format!("\\text{{{}}}", &rest[..end]), end + 2))
}

/// Converts a backslash command, handling the Word-specific spellings.
fn convert_command(text: &str, start: usize) -> Option<(String, usize, bool)> {
    let name_start = start + 1;
    let mut name_end = name_start;
    while name_end < text.len() && text.as_bytes()[name_end].is_ascii_alphabetic() {
        name_end += 1;
    }
    if name_end == name_start {
        return None;
    }
    let name = &text[name_start..name_end];
    match name {
        "eqarray" => {
            let (inner, used) = read_group(&text[name_end..], '(', ')')?;
            Some((
                array_from_rows(inner, ArrayKind::EquationArray),
                name_end + used,
                true,
            ))
        }
        "matrix" => {
            let (inner, used) = read_group(&text[name_end..], '(', ')')?;
            Some((
                array_from_rows(inner, ArrayKind::Matrix),
                name_end + used,
                true,
            ))
        }
        "sqrt" => convert_sqrt_command(text, name_end),
        "rect" => convert_parenthesised(text, name_end, "boxed"),
        "overbar" => convert_parenthesised(text, name_end, "overline"),
        "underbar" => convert_parenthesised(text, name_end, "underline"),
        "vec" | "hat" | "bar" | "tilde" | "overbrace" | "underbrace" => {
            convert_parenthesised(text, name_end, name)
        }
        _ => Some((format!("\\{name}"), name_end, false)),
    }
}

/// Handles `\sqrt(x)`, `\sqrt[n](x)` and their braced LaTeX spellings.
fn convert_sqrt_command(text: &str, name_end: usize) -> Option<(String, usize, bool)> {
    let (degree, body_start) = match read_group(&text[name_end..], '[', ']') {
        Some((degree, used)) => (Some(convert_segment(degree).latex), name_end + used),
        None => (None, name_end),
    };
    let group = read_group(&text[body_start..], '(', ')')
        .or_else(|| read_group(&text[body_start..], '{', '}'));
    let Some((body, used)) = group else {
        return Some(("\\sqrt".to_owned(), name_end, false));
    };
    let body = convert_segment(body).latex;
    let latex = match degree {
        Some(degree) => format!("\\sqrt[{degree}]{{{body}}}"),
        None => format!("\\sqrt{{{body}}}"),
    };
    Some((latex, body_start + used, true))
}

/// Rewrites `\name(arg)` as `\target{arg}`, the LaTeX accent spelling.
fn convert_parenthesised(
    text: &str,
    name_end: usize,
    target: &str,
) -> Option<(String, usize, bool)> {
    let Some((body, used)) = read_group(&text[name_end..], '(', ')') else {
        return Some((format!("\\{target}"), name_end, false));
    };
    Some((
        format!("\\{target}{{{}}}", convert_segment(body).latex),
        name_end + used,
        true,
    ))
}

/// Which linear command produced an array; it selects the LaTeX environment.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ArrayKind {
    /// `█(...)` / `\eqarray(...)`, rendered as `aligned` or `gather`.
    EquationArray,
    /// `■(...)` / `\matrix(...)`, rendered as a matrix with parentheses.
    Matrix,
}

/// Builds the LaTeX body of an array from its linear rows.
fn array_from_rows(inner: &str, kind: ArrayKind) -> String {
    let rows = split_rows(inner);
    let cells = rows
        .iter()
        .map(|row| split_top_level(row, CELL_SEPARATOR))
        .collect::<Vec<_>>();
    let environment = match kind {
        ArrayKind::EquationArray if cells.iter().any(|row| row.len() > 1) => "aligned",
        ArrayKind::EquationArray => "gather",
        ArrayKind::Matrix => "pmatrix",
    };
    array_latex(environment, &cells)
}

fn array_latex(environment: &str, rows: &[Vec<&str>]) -> String {
    let mut latex = format!("\\begin{{{environment}}}");
    for (row_index, row) in rows.iter().enumerate() {
        if row_index > 0 {
            latex.push_str("\\\\");
        }
        for (cell_index, cell) in row.iter().enumerate() {
            if cell_index > 0 {
                latex.push('&');
            }
            latex.push_str(convert_segment(cell.trim()).latex.trim());
        }
    }
    latex.push_str(&format!("\\end{{{environment}}}"));
    latex
}

/// Splits linear rows on `@`, falling back to the LaTeX `\\` separator.
fn split_rows(inner: &str) -> Vec<&str> {
    let rows = split_top_level(inner, ROW_SEPARATOR);
    if rows.len() > 1 {
        return rows;
    }
    split_on_double_backslash(inner)
}

fn split_top_level(text: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut head = 0usize;
    let mut depth = 0i32;
    let mut quoted = false;
    let mut escaped = false;
    for (offset, character) in text.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' => {
                escaped = true;
                continue;
            }
            '"' => {
                quoted = !quoted;
                continue;
            }
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
        if character == separator && depth == 0 && !quoted {
            parts.push(&text[head..offset]);
            head = offset + character.len_utf8();
        }
    }
    parts.push(&text[head..]);
    parts
}

fn split_on_double_backslash(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut parts = Vec::new();
    let mut head = 0usize;
    let mut depth = 0i32;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'\\' if depth == 0 && bytes.get(index + 1) == Some(&b'\\') => {
                parts.push(&text[head..index]);
                index += 2;
                head = index;
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    parts.push(&text[head..]);
    parts
}

/// Reads a balanced group starting at `opener`; returns the inner text and the
/// number of bytes consumed including both delimiters.
fn read_group(text: &str, opener: char, closer: char) -> Option<(&str, usize)> {
    let first = text.chars().next()?;
    if first != opener {
        return None;
    }
    if opener == closer {
        let inner_start = opener.len_utf8();
        let end = text[inner_start..].find(closer)? + inner_start;
        return Some((&text[inner_start..end], end + closer.len_utf8()));
    }
    let mut depth = 1usize;
    let index = opener.len_utf8();
    for (offset, character) in text[index..].char_indices() {
        if character == opener {
            depth += 1;
        } else if character == closer {
            depth -= 1;
            if depth == 0 {
                let inner = &text[index..index + offset];
                return Some((inner, index + offset + character.len_utf8()));
            }
        }
    }
    None
}

fn matching_closer(opener: char) -> Option<char> {
    match opener {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '|' => Some('|'),
        '‖' => Some('‖'),
        _ => None,
    }
}

fn matrix_environment(opener: char) -> &'static str {
    match opener {
        '(' => "pmatrix",
        '[' => "bmatrix",
        '{' => "Bmatrix",
        '|' => "vmatrix",
        '‖' => "Vmatrix",
        _ => "matrix",
    }
}

/// Maps the Unicode characters Word leaves in linear text to LaTeX commands.
fn symbol_latex(character: char) -> Option<&'static str> {
    let latex = match character {
        'α' => "\\alpha",
        'β' => "\\beta",
        'γ' => "\\gamma",
        'δ' => "\\delta",
        'ε' => "\\varepsilon",
        'ϵ' => "\\epsilon",
        'ζ' => "\\zeta",
        'η' => "\\eta",
        'θ' => "\\theta",
        'ϑ' => "\\vartheta",
        'ι' => "\\iota",
        'κ' => "\\kappa",
        'λ' => "\\lambda",
        'μ' => "\\mu",
        'ν' => "\\nu",
        'ξ' => "\\xi",
        'π' => "\\pi",
        'ϖ' => "\\varpi",
        'ρ' => "\\rho",
        'ϱ' => "\\varrho",
        'σ' => "\\sigma",
        'ς' => "\\varsigma",
        'τ' => "\\tau",
        'υ' => "\\upsilon",
        'φ' => "\\varphi",
        'ϕ' => "\\phi",
        'χ' => "\\chi",
        'ψ' => "\\psi",
        'ω' => "\\omega",
        'Γ' => "\\Gamma",
        'Δ' => "\\Delta",
        'Θ' => "\\Theta",
        'Λ' => "\\Lambda",
        'Ξ' => "\\Xi",
        'Π' => "\\Pi",
        'Σ' => "\\Sigma",
        'Υ' => "\\Upsilon",
        'Φ' => "\\Phi",
        'Ψ' => "\\Psi",
        'Ω' => "\\Omega",
        'Α' => "A",
        'Β' => "B",
        'Ε' => "E",
        'Ζ' => "Z",
        'Η' => "H",
        'Ι' => "I",
        'Κ' => "K",
        'Μ' => "M",
        'Ν' => "N",
        'Ο' => "O",
        'Ρ' => "P",
        'Τ' => "T",
        'Χ' => "X",
        '≤' => "\\le",
        '≥' => "\\ge",
        '≠' => "\\ne",
        '≈' => "\\approx",
        '≡' => "\\equiv",
        '∼' => "\\sim",
        '≅' => "\\cong",
        '∝' => "\\propto",
        '≪' => "\\ll",
        '≫' => "\\gg",
        '×' => "\\times",
        '⋅' | '·' => "\\cdot",
        '±' => "\\pm",
        '∓' => "\\mp",
        '÷' => "\\div",
        '∘' => "\\circ",
        '∗' => "\\ast",
        '⋆' => "\\star",
        '−' => "-",
        '∑' => "\\sum",
        '∏' => "\\prod",
        '∐' => "\\coprod",
        '⋃' => "\\bigcup",
        '⋂' => "\\bigcap",
        '∫' => "\\int",
        '∬' => "\\iint",
        '∭' => "\\iiint",
        '∮' => "\\oint",
        '∞' => "\\infty",
        '∂' => "\\partial",
        '∇' => "\\nabla",
        'ℏ' => "\\hbar",
        'ℵ' => "\\aleph",
        '∈' => "\\in",
        '∉' => "\\notin",
        '∋' => "\\ni",
        '⊂' => "\\subset",
        '⊆' => "\\subseteq",
        '⊃' => "\\supset",
        '⊇' => "\\supseteq",
        '∪' => "\\cup",
        '∩' => "\\cap",
        '∖' => "\\setminus",
        '∅' => "\\emptyset",
        '∀' => "\\forall",
        '∃' => "\\exists",
        '∄' => "\\nexists",
        '¬' => "\\neg",
        '∧' => "\\land",
        '∨' => "\\lor",
        '∣' => "\\mid",
        '∥' => "\\parallel",
        '⊥' => "\\perp",
        '→' => "\\to",
        '←' => "\\leftarrow",
        '↔' => "\\leftrightarrow",
        '⇒' => "\\Rightarrow",
        '⇐' => "\\Leftarrow",
        '⇔' => "\\iff",
        '⟶' => "\\longrightarrow",
        '⟹' => "\\Longrightarrow",
        '↦' => "\\mapsto",
        '⟨' => "\\langle",
        '⟩' => "\\rangle",
        '…' => "\\ldots",
        '⋯' => "\\cdots",
        '⋮' => "\\vdots",
        '⋱' => "\\ddots",
        _ => return None,
    };
    Some(latex)
}
