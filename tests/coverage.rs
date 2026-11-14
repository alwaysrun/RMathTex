//! Capability coverage matrix: Core formulas must pass all four output chains.

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

use eframe::egui;

use convert::check_capability;
use model::{CoverageLevel, Delimiter, ExportStyle, RenderKey};
use render::Renderer;

/// Appendix A.2 baseline plus the A.3 representatives.
const CORE_FORMULAS: &[&str] = &[
    // A.2.1 basic structure
    "x^{10}",
    "x_i",
    "x_{i}^{j}",
    "a+b-c",
    "\\frac{a}{b}",
    "\\sqrt{x}",
    "\\sqrt[3]{x}",
    "\\binom{n}{k}",
    "\\left(\\frac{a}{b}\\right)",
    "a\\,b\\:c\\;d\\quad e\\!f",
    "\\{x\\}",
    // A.2.2 fonts and text
    "\\text{if } x>0",
    "\\mathrm{d}x",
    "\\mathbf{A}",
    "\\mathit{x}",
    "\\mathsf{T}",
    "\\mathtt{x}",
    "\\mathbb{R}",
    "\\mathcal{N}",
    "\\mathscr{L}",
    "\\mathfrak{g}",
    "\\operatorname{Re}(z)",
    "\\boldsymbol{\\alpha}",
    // A.2.3 Greek and symbols
    "\\alpha\\beta\\gamma\\delta\\epsilon\\varepsilon\\zeta\\eta\\theta\\vartheta\\iota\\kappa",
    "\\lambda\\mu\\nu\\xi\\pi\\rho\\sigma\\tau\\upsilon\\phi\\varphi\\chi\\psi\\omega",
    "\\Gamma\\Delta\\Theta\\Lambda\\Xi\\Pi\\Sigma\\Upsilon\\Phi\\Psi\\Omega",
    "\\infty\\partial\\nabla\\hbar\\aleph",
    "a\\le b\\ge c\\ne d\\approx e\\equiv f\\sim g\\propto h\\ll i\\gg j",
    "a\\times b\\cdot c\\pm d\\mp e\\div f\\circ g\\ast h\\star i",
    "a\\to b\\rightarrow c\\Rightarrow d\\iff e\\mapsto f\\longrightarrow g",
    // A.2.4 sets and logic
    "a\\in A\\notin B\\ni C",
    "A\\subset B\\subseteq C\\supset D\\cup E\\cap F\\setminus G",
    "\\emptyset\\varnothing",
    "\\forall x\\exists y\\nexists z\\neg p\\land q\\lor r\\implies s",
    "a\\mid b\\parallel c\\perp d",
    // A.2.5 operators and calculus
    "\\sum_{i=1}^{n} i",
    "\\prod_{i=1}^{n} i",
    "\\coprod_{i=1}^{n} A_i",
    "\\bigcup_{i=1}^{n} A_i",
    "\\bigcap_{i=1}^{n} A_i",
    "\\int_{a}^{b} f(x)\\,\\mathrm{d}x",
    "\\iint_D f",
    "\\iiint_V f",
    "\\oint_\\gamma f",
    "\\lim_{x\\to 0} \\frac{\\sin x}{x}",
    "\\max_{x} f\\quad\\min_{x} f\\quad\\sup_{x} f\\quad\\inf_{x} f",
    // A.2.6 matrices
    "\\begin{matrix} 1 & 2 \\\\ 3 & 4 \\end{matrix}",
    "\\begin{pmatrix} 1 & 2 \\\\ 3 & 4 \\end{pmatrix}",
    "\\begin{bmatrix} 1 & 2 \\\\ 3 & 4 \\end{bmatrix}",
    "\\begin{vmatrix} 1 & 2 \\\\ 3 & 4 \\end{vmatrix}",
    // A.2.7 functions and decorations
    "\\sin x\\cos x\\tan x\\cot x\\sec x\\csc x\\arcsin x\\arctan x\\sinh x\\cosh x\\tanh x",
    "\\log x\\ln x\\exp x\\det A\\dim V\\ker T\\deg p\\gcd(a,b)\\Pr(A)\\arg z",
    "\\hat{x}\\bar{x}\\vec{x}\\tilde{x}\\dot{x}\\ddot{x}",
    "\\overline{AB}\\underline{x}\\widehat{xy}\\widetilde{xy}",
    "\\overbrace{a+b}\\underbrace{c+d}",
    "\\overset{a}{=}\\underset{b}{=}\\stackrel{c}{=}",
    "\\boxed{E=mc^2}",
    "a\\not= b",
    // A.3 representatives
    "\\lim_{x\\to 0}\\frac{\\sin x}{x}=1",
    "\\int_{0}^{\\infty} e^{-x^{2}}\\,\\mathrm{d}x=\\frac{\\sqrt{\\pi}}{2}",
    "\\frac{\\partial f}{\\partial x}",
    "\\sum_{n=1}^{\\infty}\\frac{1}{n^{2}}=\\frac{\\pi^{2}}{6}",
    "A\\mathbf{x}=\\lambda\\mathbf{x}",
    "\\det(A-\\lambda I)=0",
    "\\langle u,v\\rangle",
    "\\mathbb{E}[X]=\\int x f(x)\\,\\mathrm{d}x",
    "\\mathbb{P}(A\\mid B)",
    "X\\sim\\mathcal{N}(\\mu,\\sigma^{2})",
    "\\binom{n}{k}p^{k}(1-p)^{n-k}",
    "y''+p(x)y'+q(x)y=0",
    "\\frac{\\partial u}{\\partial t}=\\Delta u",
    "\\nabla\\cdot\\mathbf{F}",
    "\\oint_{\\gamma} f(z)\\,\\mathrm{d}z",
    "\\forall x\\in\\mathbb{R},\\ \\exists y:\\ y>x",
    "A\\subseteq B \\Rightarrow A\\cup B=B",
    "\\mathbb{N}\\subset\\mathbb{Z}\\subset\\mathbb{Q}\\subset\\mathbb{R}",
    "e^{i\\theta}=\\cos\\theta+i\\sin\\theta",
    "z=x+iy\\in\\mathbb{C}",
    "\\varepsilon>0",
    "O(n^{2})",
    "f(x)\\approx f(x_{0})+f'(x_{0})(x-x_{0})",
    "f(x)=\\sum_{n=0}^{\\infty}\\frac{f^{(n)}(a)}{n!}(x-a)^{n}",
];

const EXTENDED_FORMULAS: &[&str] = &[
    "\\begin{aligned} a &= b \\\\ c &= d \\end{aligned}",
    "\\begin{cases} 1 & x>0 \\\\ 0 & x\\le 0 \\end{cases}",
    "\\textcolor{red}{x^2}",
    "x^{a^{b^{c^{d}}}}",
    "\\big( x \\big)",
    "\\Big[ x \\Big]",
    "\\bigg\\{ x \\bigg\\}",
    "\\Bigg\\langle x \\Bigg\\rangle",
    "\\oiint_S \\mathbf{F}\\cdot\\mathrm{d}\\mathbf{S}",
];

const UNSUPPORTED_FORMULAS: &[&str] = &[
    "\\tikz \\draw (0,0)--(1,1);",
    "\\ce{H2O}",
    "\\newcommand{\\x}{1}",
    "\\section{Intro} x",
    "\\begin{document} x \\end{document}",
];

/// One formula per university-math category (appendix A.3).
const UNIVERSITY_FORMULAS: &[&str] = &[
    "\\lim_{x\\to 0}\\frac{\\sin x}{x}=1",
    "\\begin{bmatrix} 1 & 2 \\\\ 3 & 4 \\end{bmatrix}",
    "X\\sim\\mathcal{N}(\\mu,\\sigma^{2})",
    "\\frac{\\partial u}{\\partial t}=\\Delta u",
    "\\forall x\\in\\mathbb{R},\\ \\exists y:\\ y>x",
    "e^{i\\theta}=\\cos\\theta+i\\sin\\theta",
    "\\arg\\min_{x\\in\\mathbb{R}^{n}} f(x)",
    "\\int_{0}^{\\infty} e^{-x^{2}}\\,\\mathrm{d}x=\\frac{\\sqrt{\\pi}}{2}",
];

fn four_chain_failures(formulas: &[&str]) -> Vec<String> {
    let renderer = match Renderer::new() {
        Ok(renderer) => renderer,
        Err(error) => return vec![format!("font: {error}")],
    };
    let style = ExportStyle::default();
    let mut failures = Vec::new();
    for formula in formulas {
        let key = RenderKey::new(formula, 14, true, 1.0, [0, 0, 0]);
        if let Err(error) = renderer.render(formula, key, egui::Pos2::ZERO) {
            failures.push(format!("preview  | {formula} | {error}"));
        }
        if let Err(error) = convert::to_mathml(formula, true) {
            failures.push(format!("mathml   | {formula} | {error}"));
        }
        if let Err(error) = convert::to_omml(formula) {
            failures.push(format!("omml     | {formula} | {error}"));
        }
        if let Err(error) = export::svg(formula, &style, renderer.font()) {
            failures.push(format!("svg      | {formula} | {error}"));
        }
        if let Err(error) = export::png(formula, &style, renderer.font()) {
            failures.push(format!("png      | {formula} | {error}"));
        }
    }
    failures
}

#[test]
fn core_set_all_four_chains() {
    let failures = four_chain_failures(CORE_FORMULAS);
    assert!(
        failures.is_empty(),
        "{} failures out of {} formulas:\n{}",
        failures.len(),
        CORE_FORMULAS.len(),
        failures.join("\n")
    );
}

#[test]
fn extended_degrades_with_notice() {
    let renderer = Renderer::new().expect("font");
    let style = ExportStyle::default();
    for formula in EXTENDED_FORMULAS {
        let coverage = check_capability(formula);
        assert_eq!(
            coverage.level,
            CoverageLevel::Extended,
            "{formula} classified as {:?}",
            coverage
        );
        let key = RenderKey::new(formula, 14, true, 1.0, [0, 0, 0]);
        renderer
            .render(formula, key, egui::Pos2::ZERO)
            .unwrap_or_else(|error| panic!("preview {formula}: {error}"));
        export::svg(formula, &style, renderer.font())
            .unwrap_or_else(|error| panic!("svg {formula}: {error}"));
        assert!(!model::to_markdown(formula, Delimiter::Display).is_empty());
    }
}

#[test]
fn unsupported_returns_err() {
    let renderer = Renderer::new().expect("font");
    for formula in UNSUPPORTED_FORMULAS {
        let coverage = check_capability(formula);
        assert_eq!(
            coverage.level,
            CoverageLevel::Unsupported,
            "{formula} classified as {coverage:?}"
        );
        assert!(
            !coverage.blocked.is_empty(),
            "{formula} has no blocked entry"
        );
        let mathml = convert::to_mathml(formula, false).expect_err("to_mathml must fail");
        assert_eq!(mathml.code, "E302");
        let omml = convert::to_omml(formula).expect_err("to_omml must fail");
        assert_eq!(omml.code, "E302");
        let key = RenderKey::new(formula, 14, false, 1.0, [0, 0, 0]);
        assert!(
            renderer.render(formula, key, egui::Pos2::ZERO).is_err(),
            "{formula} must not produce shapes"
        );
    }
}

#[test]
fn markdown_delimiter_cases() {
    struct Case {
        input: &'static str,
        body: &'static str,
        delimiter: Delimiter,
        style_hint: Option<bool>,
        warning: bool,
    }
    let cases = [
        Case {
            input: "$x^2$",
            body: "x^2",
            delimiter: Delimiter::Inline,
            style_hint: Some(false),
            warning: false,
        },
        Case {
            input: "$$\\frac{a}{b}$$",
            body: "\\frac{a}{b}",
            delimiter: Delimiter::Display,
            style_hint: Some(true),
            warning: false,
        },
        Case {
            input: "\\(a+b\\)",
            body: "a+b",
            delimiter: Delimiter::MacroInline,
            style_hint: Some(false),
            warning: false,
        },
        Case {
            input: "\\[\\int_0^1 x\\,dx\\]",
            body: "\\int_0^1 x\\,dx",
            delimiter: Delimiter::MacroDisplay,
            style_hint: Some(true),
            warning: false,
        },
        Case {
            input: "```math\nx^2\n```",
            body: "x^2",
            delimiter: Delimiter::Fenced,
            style_hint: Some(true),
            warning: false,
        },
        Case {
            input: "\\frac{1}{2}",
            body: "\\frac{1}{2}",
            delimiter: Delimiter::None,
            style_hint: None,
            warning: false,
        },
        Case {
            input: "$10 and \\$5",
            body: "$10 and \\$5",
            delimiter: Delimiter::None,
            style_hint: None,
            warning: false,
        },
        Case {
            input: "$ x^2 $",
            body: "$ x^2 $",
            delimiter: Delimiter::None,
            style_hint: None,
            warning: false,
        },
        Case {
            input: "$\\frac{1}{2}",
            body: "$\\frac{1}{2}",
            delimiter: Delimiter::None,
            style_hint: None,
            warning: true,
        },
        Case {
            input: "$a$ and $b$",
            body: "$a$ and $b$",
            delimiter: Delimiter::None,
            style_hint: None,
            warning: true,
        },
    ];
    for case in cases {
        let normalized = model::normalize_source(case.input);
        assert_eq!(normalized.body, case.body, "body of {:?}", case.input);
        assert_eq!(
            normalized.delimiter, case.delimiter,
            "delimiter of {:?}",
            case.input
        );
        assert_eq!(
            normalized.style_hint, case.style_hint,
            "style hint of {:?}",
            case.input
        );
        assert_eq!(
            normalized.warning.is_some(),
            case.warning,
            "warning of {:?}",
            case.input
        );
        assert_eq!(normalized.raw, case.input, "raw must stay verbatim");
    }
}

#[test]
fn capability_scan() {
    let core = [
        "x^2",
        "\\frac{a}{b}",
        "\\begin{bmatrix}1&2\\\\3&4\\end{bmatrix}",
        "\\left(\\frac{a}{b}\\right)",
    ];
    for formula in core {
        assert_eq!(
            check_capability(formula).level,
            CoverageLevel::Core,
            "{formula}"
        );
    }
    let extended = [
        "a & b",
        "a \\\\ b",
        "\\begin{aligned} a &= b \\end{aligned}",
        "\\textcolor{red}{x}",
        "\\limits",
        "\\unknowncmd{x}",
        "\\begin{array}{cc} a & b \\end{array}",
    ];
    for formula in extended {
        assert_eq!(
            check_capability(formula).level,
            CoverageLevel::Extended,
            "{formula}"
        );
    }
    let unsupported = ["\\tikz", "\\ce{H2O}", "\\newcommand{\\x}{1}", "\\def\\x{1}"];
    for formula in unsupported {
        assert_eq!(
            check_capability(formula).level,
            CoverageLevel::Unsupported,
            "{formula}"
        );
    }
}

#[test]
fn university_eight_categories_covered() {
    assert_eq!(UNIVERSITY_FORMULAS.len(), 8);
    for formula in UNIVERSITY_FORMULAS {
        assert_eq!(
            check_capability(formula).level,
            CoverageLevel::Core,
            "{formula}"
        );
    }
    let failures = four_chain_failures(UNIVERSITY_FORMULAS);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
