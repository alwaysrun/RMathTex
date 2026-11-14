//! Third-party text conversions plus the whitelist capability scan.

use crate::model::{AppError, Coverage, CoverageLevel, MSG_UNSUPPORTED};

/// Commands accepted by all three parsers (appendix A.2 Core set).
const CORE_COMMANDS: &[&str] = &[
    "frac",
    "sqrt",
    "binom",
    "left",
    "right",
    "text",
    "mathrm",
    "mathbf",
    "mathit",
    "mathsf",
    "mathtt",
    "mathbb",
    "mathcal",
    "mathscr",
    "mathfrak",
    "operatorname",
    "boldsymbol",
    "infty",
    "partial",
    "nabla",
    "hbar",
    "aleph",
    "le",
    "leq",
    "ge",
    "geq",
    "ne",
    "neq",
    "approx",
    "equiv",
    "sim",
    "propto",
    "ll",
    "gg",
    "times",
    "cdot",
    "pm",
    "mp",
    "div",
    "circ",
    "ast",
    "star",
    "to",
    "rightarrow",
    "Rightarrow",
    "Leftarrow",
    "leftarrow",
    "iff",
    "leftrightarrow",
    "mapsto",
    "longrightarrow",
    "Longrightarrow",
    "implies",
    "impliedby",
    "in",
    "notin",
    "ni",
    "subset",
    "subseteq",
    "supset",
    "supseteq",
    "cup",
    "cap",
    "setminus",
    "emptyset",
    "varnothing",
    "forall",
    "exists",
    "nexists",
    "neg",
    "lnot",
    "land",
    "lor",
    "mid",
    "parallel",
    "perp",
    "sum",
    "prod",
    "coprod",
    "bigcup",
    "bigcap",
    "int",
    "iint",
    "iiint",
    "oint",
    "lim",
    "max",
    "min",
    "sup",
    "inf",
    "sin",
    "cos",
    "tan",
    "cot",
    "sec",
    "csc",
    "arcsin",
    "arccos",
    "arctan",
    "sinh",
    "cosh",
    "tanh",
    "log",
    "ln",
    "exp",
    "det",
    "dim",
    "ker",
    "deg",
    "gcd",
    "Pr",
    "arg",
    "hom",
    "hat",
    "bar",
    "vec",
    "tilde",
    "dot",
    "ddot",
    "check",
    "breve",
    "acute",
    "grave",
    "overline",
    "underline",
    "widehat",
    "widetilde",
    "overbrace",
    "underbrace",
    "overset",
    "underset",
    "stackrel",
    "boxed",
    "not",
    "langle",
    "rangle",
    "lvert",
    "rvert",
    "lVert",
    "rVert",
    "vert",
    "Vert",
    "quad",
    "qquad",
    "enspace",
    "thinspace",
    "ell",
    "ldots",
    "cdots",
    "dots",
    "vdots",
    "ddots",
    "dotsb",
    "alpha",
    "beta",
    "gamma",
    "delta",
    "epsilon",
    "varepsilon",
    "zeta",
    "eta",
    "theta",
    "vartheta",
    "iota",
    "kappa",
    "lambda",
    "mu",
    "nu",
    "xi",
    "omicron",
    "pi",
    "varpi",
    "rho",
    "varrho",
    "sigma",
    "varsigma",
    "tau",
    "upsilon",
    "phi",
    "varphi",
    "chi",
    "psi",
    "omega",
    "Gamma",
    "Delta",
    "Theta",
    "Lambda",
    "Xi",
    "Pi",
    "Sigma",
    "Upsilon",
    "Phi",
    "Psi",
    "Omega",
    "begin",
    "end",
];

/// Constructs that render but are outside the three-parser intersection.
///
/// The size modifiers and `\oiint` are listed here because their OMML
/// conversion is not available in the pinned `easydoc-math` release; appendix
/// A.6 rule 2 demotes a Core candidate as soon as one chain fails.
const EXTENDED_MARKERS: &[&str] = &[
    "textcolor",
    "color",
    "colorbox",
    "substack",
    "xrightarrow",
    "cancel",
    "tag",
    "middle",
    "phantom",
    "limits",
    "nolimits",
    "operatorname*",
    "hspace",
    "vspace",
    "big",
    "Big",
    "bigg",
    "Bigg",
    "oiint",
];

/// Constructs that are rejected outright (appendix A.5).
const UNSUPPORTED_MARKERS: &[&str] = &[
    "tikz",
    "tikzpicture",
    "pgf",
    "ce",
    "mhchem",
    "chemfig",
    "section",
    "subsection",
    "subsubsection",
    "paragraph",
    "documentclass",
    "usepackage",
    "newcommand",
    "renewcommand",
    "providecommand",
    "def",
    "newenvironment",
    "input",
    "include",
    "includegraphics",
    "title",
    "author",
    "maketitle",
    "label",
    "ref",
    "cite",
    "bibliography",
    "bibliographystyle",
    "document",
];

/// Environment names accepted by all three parsers.
const CORE_ENVIRONMENTS: &[&str] = &[
    "matrix", "pmatrix", "bmatrix", "Bmatrix", "vmatrix", "Vmatrix",
];

/// Multi-line environments: preview and exports work, Office paths are best effort.
const EXTENDED_ENVIRONMENTS: &[&str] = &[
    "aligned",
    "align",
    "align*",
    "gather",
    "gather*",
    "split",
    "array",
    "cases",
    "equation",
    "equation*",
    "eqnarray",
    "multline",
    "multline*",
    "gathered",
];

/// Environment names rejected outright.
const UNSUPPORTED_ENVIRONMENTS: &[&str] =
    &["tikzpicture", "document", "tabular", "figure", "table"];

/// Label reported for a command that appears in no table.
const UNLISTED_COMMAND: &str = "未收录命令";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Core,
    Extended,
    Unsupported,
    Unlisted,
}

/// Converts a normalized body to Presentation MathML.
pub fn to_mathml(latex: &str, display: bool) -> Result<String, AppError> {
    reject_unsupported(latex)?;
    let style = if display {
        latex2mathml::DisplayStyle::Block
    } else {
        latex2mathml::DisplayStyle::Inline
    };
    latex2mathml::latex_to_mathml(latex, style)
        .map_err(|error| AppError::caused("E301", "MathML 转换失败", error))
}

/// Converts a normalized body to a complete `<m:oMath>` element.
pub fn to_omml(latex: &str) -> Result<String, AppError> {
    reject_unsupported(latex)?;
    easydoc_math::latex_to_omml::convert(latex)
        .map_err(|error| AppError::caused("E303", "OMML 转换失败", error))
}

/// Converts source rows into one MathML table so Office keeps hard line breaks.
///
/// Word maps `<mtable>` to a multi-row equation, whereas a flat `<math>` would
/// fold the newlines into spaces. Blank rows stay as empty cells so the copied
/// gap survives the import.
pub fn rows_to_mathml(rows: &[&str], display: bool) -> Result<String, AppError> {
    let mut table = String::from("<mtable>");
    for row in rows {
        table.push_str("<mtr><mtd>");
        if !row.is_empty() {
            let mathml = to_mathml(row, display)?;
            table.push_str(mathml_inner(&mathml));
        }
        table.push_str("</mtd></mtr>");
    }
    table.push_str("</mtable>");
    let display = if display { "block" } else { "inline" };
    Ok(format!(
        r#"<math xmlns="http://www.w3.org/1998/Math/MathML" display="{display}">{table}</math>"#
    ))
}

/// Converts source rows into one `<m:eqArr>`, Word's native multi-line form.
pub fn rows_to_omml(rows: &[&str]) -> Result<String, AppError> {
    let mut array = String::from("<m:eqArr>");
    for row in rows {
        array.push_str("<m:e>");
        if !row.is_empty() {
            array.push_str(omml_inner(&to_omml(row)?));
        }
        array.push_str("</m:e>");
    }
    array.push_str("</m:eqArr>");
    Ok(format!("<m:oMath>{array}</m:oMath>"))
}

/// Converts an `<m:oMath>` element back to LaTeX.
pub fn from_omml(omml: &str) -> Result<String, AppError> {
    easydoc_math::omml_to_latex::convert(omml)
        .map_err(|error| AppError::caused("E304", "OMML 还原 LaTeX 失败", error))
}

/// Keeps the inner content of `<m:oMath>` for callers that own the root.
pub(crate) fn omml_inner(omml: &str) -> &str {
    let trimmed = omml.trim();
    if !trimmed.starts_with("<m:oMath") {
        return trimmed;
    }
    let Some(open_end) = trimmed.find('>') else {
        return trimmed;
    };
    let body = &trimmed[open_end + 1..];
    body.strip_suffix("</m:oMath>").unwrap_or(body)
}

/// Keeps the inner content of `<math>` so callers can rebuild the root.
fn mathml_inner(mathml: &str) -> &str {
    let trimmed = mathml.trim();
    let Some(open_end) = trimmed.find('>') else {
        return trimmed;
    };
    let body = &trimmed[open_end + 1..];
    body.strip_suffix("</math>").unwrap_or(body)
}

/// Scans a normalized body against the three static tables.
#[must_use]
pub fn check_capability(body: &str) -> Coverage {
    let scan = scan(body);
    let mut blocked: Vec<&'static str> = Vec::new();
    let mut level = CoverageLevel::Core;
    for (name, class) in scan.commands.iter().chain(scan.environments.iter()) {
        match class {
            Class::Unsupported => return unsupported(blocked_with(&mut blocked, name)),
            Class::Extended | Class::Unlisted => {
                level = CoverageLevel::Extended;
                push_unique(&mut blocked, name);
            }
            Class::Core => {}
        }
    }
    if scan.deep_script || scan.loose_separator {
        level = CoverageLevel::Extended;
    }
    Coverage { level, blocked }
}

fn unsupported(blocked: Vec<&'static str>) -> Coverage {
    Coverage {
        level: CoverageLevel::Unsupported,
        blocked,
    }
}

fn blocked_with(blocked: &mut Vec<&'static str>, name: &'static str) -> Vec<&'static str> {
    push_unique(blocked, name);
    blocked.clone()
}

fn push_unique(blocked: &mut Vec<&'static str>, name: &'static str) {
    if !blocked.contains(&name) {
        blocked.push(name);
    }
}

fn reject_unsupported(body: &str) -> Result<(), AppError> {
    let coverage = check_capability(body);
    if coverage.level == CoverageLevel::Unsupported {
        return Err(AppError::new(
            "E302",
            MSG_UNSUPPORTED.replace("{commands}", &coverage.blocked.join(", ")),
        ));
    }
    Ok(())
}

struct Scan {
    commands: Vec<(&'static str, Class)>,
    environments: Vec<(&'static str, Class)>,
    deep_script: bool,
    loose_separator: bool,
}

fn scan(body: &str) -> Scan {
    let bytes = body.as_bytes();
    let mut scan = Scan {
        commands: Vec::new(),
        environments: Vec::new(),
        deep_script: false,
        loose_separator: false,
    };
    let mut depth = 0usize;
    let mut matrix_depth = 0i32;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => {
                let start = index + 1;
                let mut end = start;
                while end < bytes.len() && bytes[end].is_ascii_alphabetic() {
                    end += 1;
                }
                if end == start {
                    // `\\` is the row separator; only a matrix may use it.
                    if bytes.get(start) == Some(&b'\\') && matrix_depth == 0 {
                        scan.loose_separator = true;
                    }
                    index = start + 1;
                    continue;
                }
                let name = &body[start..end];
                if matches!(name, "begin" | "end") {
                    if let Some((environment, next)) = read_environment(body, end) {
                        let (label, class) = classify_environment(environment);
                        if name == "begin" {
                            if class == Class::Core {
                                matrix_depth += 1;
                            }
                            scan.environments.push((label, class));
                        } else if class == Class::Core {
                            matrix_depth = (matrix_depth - 1).max(0);
                        }
                        index = next;
                        continue;
                    }
                }
                let star = bytes.get(end) == Some(&b'*');
                scan.commands.push(classify_command(name, star));
                index = end;
            }
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            b'^' | b'_' => {
                if depth > 2 {
                    scan.deep_script = true;
                }
            }
            b'&' if matrix_depth == 0 => scan.loose_separator = true,
            _ => {}
        }
        index += 1;
    }
    scan
}

fn read_environment(body: &str, from: usize) -> Option<(&str, usize)> {
    let bytes = body.as_bytes();
    if bytes.get(from) != Some(&b'{') {
        return None;
    }
    let close = body[from..].find('}')? + from;
    Some((&body[from + 1..close], close + 1))
}

fn classify_command(name: &str, starred: bool) -> (&'static str, Class) {
    if name == "operatorname" && starred {
        return ("operatorname*", Class::Extended);
    }
    for candidate in CORE_COMMANDS {
        if *candidate == name {
            return (candidate, Class::Core);
        }
    }
    for candidate in EXTENDED_MARKERS {
        if *candidate == name {
            return (candidate, Class::Extended);
        }
    }
    for candidate in UNSUPPORTED_MARKERS {
        if *candidate == name {
            return (candidate, Class::Unsupported);
        }
    }
    (UNLISTED_COMMAND, Class::Unlisted)
}

fn classify_environment(name: &str) -> (&'static str, Class) {
    for candidate in CORE_ENVIRONMENTS {
        if *candidate == name {
            return (candidate, Class::Core);
        }
    }
    for candidate in EXTENDED_ENVIRONMENTS {
        if *candidate == name {
            return (candidate, Class::Extended);
        }
    }
    for candidate in UNSUPPORTED_ENVIRONMENTS {
        if *candidate == name {
            return (candidate, Class::Unsupported);
        }
    }
    (UNLISTED_COMMAND, Class::Unlisted)
}
