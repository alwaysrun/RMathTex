//! Static formula catalog shared by the formula menu and the favorites toolbar.
//!
//! Every entry carries a stable `id` used by the persisted favorites list, a
//! short Chinese label for compact buttons, and the LaTeX snippet inserted into
//! the editor. The catalog is the single source of truth for both UI surfaces.

/// One insertable formula snippet.
pub struct FormulaEntry {
    /// Stable identifier persisted in the configuration file.
    pub id: &'static str,
    /// Short label used by toolbar buttons and menu items.
    pub label: &'static str,
    /// LaTeX source inserted at the editor cursor.
    pub latex: &'static str,
}

/// One formula group shown as a submenu.
pub struct FormulaCategory {
    /// Submenu title.
    pub name: &'static str,
    /// Entries belonging to this group.
    pub entries: &'static [FormulaEntry],
}

/// Categories and entries of the formula menu, in display order.
pub const FORMULA_CATEGORIES: &[FormulaCategory] = &[
    FormulaCategory {
        name: "基础结构",
        entries: &[
            FormulaEntry {
                id: "frac",
                label: "分式",
                latex: r"\frac{a}{b}",
            },
            FormulaEntry {
                id: "sqrt",
                label: "根式",
                latex: r"\sqrt{x}",
            },
            FormulaEntry {
                id: "nth_root",
                label: "n 次根",
                latex: r"\sqrt[n]{x}",
            },
            FormulaEntry {
                id: "sup",
                label: "上标",
                latex: r"x^{2}",
            },
            FormulaEntry {
                id: "sub",
                label: "下标",
                latex: r"x_{i}",
            },
            FormulaEntry {
                id: "sub_sup",
                label: "上下标",
                latex: r"x_{i}^{2}",
            },
            FormulaEntry {
                id: "paren",
                label: "括号",
                latex: r"\left( x \right)",
            },
            FormulaEntry {
                id: "abs",
                label: "绝对值",
                latex: r"\left| x \right|",
            },
            FormulaEntry {
                id: "norm",
                label: "范数",
                latex: r"\left\Vert x \right\Vert",
            },
            FormulaEntry {
                id: "brace",
                label: "花括号",
                latex: r"\left\{ x \right\}",
            },
            FormulaEntry {
                id: "binom",
                label: "组合数",
                latex: r"\binom{n}{k}",
            },
        ],
    },
    FormulaCategory {
        name: "希腊字母",
        entries: &[
            FormulaEntry {
                id: "alpha",
                label: "α",
                latex: r"\alpha",
            },
            FormulaEntry {
                id: "beta",
                label: "β",
                latex: r"\beta",
            },
            FormulaEntry {
                id: "gamma",
                label: "γ",
                latex: r"\gamma",
            },
            FormulaEntry {
                id: "delta",
                label: "δ",
                latex: r"\delta",
            },
            FormulaEntry {
                id: "epsilon",
                label: "ε",
                latex: r"\epsilon",
            },
            FormulaEntry {
                id: "varepsilon",
                label: "ε 变体",
                latex: r"\varepsilon",
            },
            FormulaEntry {
                id: "theta",
                label: "θ",
                latex: r"\theta",
            },
            FormulaEntry {
                id: "lambda",
                label: "λ",
                latex: r"\lambda",
            },
            FormulaEntry {
                id: "mu",
                label: "μ",
                latex: r"\mu",
            },
            FormulaEntry {
                id: "pi",
                label: "π",
                latex: r"\pi",
            },
            FormulaEntry {
                id: "rho",
                label: "ρ",
                latex: r"\rho",
            },
            FormulaEntry {
                id: "sigma",
                label: "σ",
                latex: r"\sigma",
            },
            FormulaEntry {
                id: "phi",
                label: "φ",
                latex: r"\phi",
            },
            FormulaEntry {
                id: "varphi",
                label: "φ 变体",
                latex: r"\varphi",
            },
            FormulaEntry {
                id: "psi",
                label: "ψ",
                latex: r"\psi",
            },
            FormulaEntry {
                id: "omega",
                label: "ω",
                latex: r"\omega",
            },
            FormulaEntry {
                id: "Gamma",
                label: "Γ",
                latex: r"\Gamma",
            },
            FormulaEntry {
                id: "Delta",
                label: "Δ",
                latex: r"\Delta",
            },
            FormulaEntry {
                id: "Theta",
                label: "Θ",
                latex: r"\Theta",
            },
            FormulaEntry {
                id: "Lambda",
                label: "Λ",
                latex: r"\Lambda",
            },
            FormulaEntry {
                id: "Sigma",
                label: "Σ",
                latex: r"\Sigma",
            },
            FormulaEntry {
                id: "Phi",
                label: "Φ",
                latex: r"\Phi",
            },
            FormulaEntry {
                id: "Omega",
                label: "Ω",
                latex: r"\Omega",
            },
        ],
    },
    FormulaCategory {
        name: "运算符与关系",
        entries: &[
            FormulaEntry {
                id: "times",
                label: "×",
                latex: r"\times",
            },
            FormulaEntry {
                id: "cdot",
                label: "·",
                latex: r"\cdot",
            },
            FormulaEntry {
                id: "pm",
                label: "±",
                latex: r"\pm",
            },
            FormulaEntry {
                id: "mp",
                label: "∓",
                latex: r"\mp",
            },
            FormulaEntry {
                id: "div",
                label: "÷",
                latex: r"\div",
            },
            FormulaEntry {
                id: "le",
                label: "≤",
                latex: r"\le",
            },
            FormulaEntry {
                id: "ge",
                label: "≥",
                latex: r"\ge",
            },
            FormulaEntry {
                id: "ne",
                label: "≠",
                latex: r"\ne",
            },
            FormulaEntry {
                id: "approx",
                label: "≈",
                latex: r"\approx",
            },
            FormulaEntry {
                id: "equiv",
                label: "≡",
                latex: r"\equiv",
            },
            FormulaEntry {
                id: "sim",
                label: "∼",
                latex: r"\sim",
            },
            FormulaEntry {
                id: "propto",
                label: "∝",
                latex: r"\propto",
            },
            FormulaEntry {
                id: "infty",
                label: "∞",
                latex: r"\infty",
            },
            FormulaEntry {
                id: "partial",
                label: "∂",
                latex: r"\partial",
            },
            FormulaEntry {
                id: "nabla",
                label: "∇",
                latex: r"\nabla",
            },
            FormulaEntry {
                id: "cdots",
                label: "⋯",
                latex: r"\cdots",
            },
            FormulaEntry {
                id: "ldots",
                label: "…",
                latex: r"\ldots",
            },
        ],
    },
    FormulaCategory {
        name: "集合与逻辑",
        entries: &[
            FormulaEntry {
                id: "in",
                label: "∈",
                latex: r"\in",
            },
            FormulaEntry {
                id: "notin",
                label: "∉",
                latex: r"\notin",
            },
            FormulaEntry {
                id: "subset",
                label: "⊂",
                latex: r"\subset",
            },
            FormulaEntry {
                id: "subseteq",
                label: "⊆",
                latex: r"\subseteq",
            },
            FormulaEntry {
                id: "supset",
                label: "⊃",
                latex: r"\supset",
            },
            FormulaEntry {
                id: "supseteq",
                label: "⊇",
                latex: r"\supseteq",
            },
            FormulaEntry {
                id: "cup",
                label: "∪",
                latex: r"\cup",
            },
            FormulaEntry {
                id: "cap",
                label: "∩",
                latex: r"\cap",
            },
            FormulaEntry {
                id: "setminus",
                label: "∖",
                latex: r"\setminus",
            },
            FormulaEntry {
                id: "emptyset",
                label: "∅",
                latex: r"\emptyset",
            },
            FormulaEntry {
                id: "forall",
                label: "∀",
                latex: r"\forall",
            },
            FormulaEntry {
                id: "exists",
                label: "∃",
                latex: r"\exists",
            },
            FormulaEntry {
                id: "neg",
                label: "¬",
                latex: r"\neg",
            },
            FormulaEntry {
                id: "land",
                label: "∧",
                latex: r"\land",
            },
            FormulaEntry {
                id: "lor",
                label: "∨",
                latex: r"\lor",
            },
            FormulaEntry {
                id: "implies",
                label: "⟹",
                latex: r"\implies",
            },
            FormulaEntry {
                id: "iff",
                label: "⟺",
                latex: r"\iff",
            },
        ],
    },
    FormulaCategory {
        name: "箭头",
        entries: &[
            FormulaEntry {
                id: "to",
                label: "→",
                latex: r"\to",
            },
            FormulaEntry {
                id: "rightarrow",
                label: "→ 长",
                latex: r"\rightarrow",
            },
            FormulaEntry {
                id: "leftarrow",
                label: "←",
                latex: r"\leftarrow",
            },
            FormulaEntry {
                id: "Rightarrow",
                label: "⇒",
                latex: r"\Rightarrow",
            },
            FormulaEntry {
                id: "Leftarrow",
                label: "⇐",
                latex: r"\Leftarrow",
            },
            FormulaEntry {
                id: "leftrightarrow",
                label: "↔",
                latex: r"\leftrightarrow",
            },
            FormulaEntry {
                id: "mapsto",
                label: "↦",
                latex: r"\mapsto",
            },
            FormulaEntry {
                id: "longrightarrow",
                label: "⟶",
                latex: r"\longrightarrow",
            },
        ],
    },
    FormulaCategory {
        name: "微积分与大型运算符",
        entries: &[
            FormulaEntry {
                id: "sum",
                label: "∑",
                latex: r"\sum_{i=1}^{n}",
            },
            FormulaEntry {
                id: "prod",
                label: "∏",
                latex: r"\prod_{i=1}^{n}",
            },
            FormulaEntry {
                id: "int",
                label: "∫",
                latex: r"\int_{a}^{b}",
            },
            FormulaEntry {
                id: "iint",
                label: "∬",
                latex: r"\iint",
            },
            FormulaEntry {
                id: "oint",
                label: "∮",
                latex: r"\oint",
            },
            FormulaEntry {
                id: "lim",
                label: "lim",
                latex: r"\lim_{x\to 0}",
            },
            FormulaEntry {
                id: "max",
                label: "max",
                latex: r"\max",
            },
            FormulaEntry {
                id: "min",
                label: "min",
                latex: r"\min",
            },
            FormulaEntry {
                id: "derivative",
                label: "导数",
                latex: r"\frac{\mathrm{d}y}{\mathrm{d}x}",
            },
            FormulaEntry {
                id: "partial_derivative",
                label: "偏导",
                latex: r"\frac{\partial f}{\partial x}",
            },
            FormulaEntry {
                id: "infinity_limit",
                label: "无穷极限",
                latex: r"\lim_{n\to\infty}",
            },
        ],
    },
    FormulaCategory {
        name: "函数与修饰",
        entries: &[
            FormulaEntry {
                id: "sin",
                label: "sin",
                latex: r"\sin x",
            },
            FormulaEntry {
                id: "cos",
                label: "cos",
                latex: r"\cos x",
            },
            FormulaEntry {
                id: "tan",
                label: "tan",
                latex: r"\tan x",
            },
            FormulaEntry {
                id: "log",
                label: "log",
                latex: r"\log_{2} x",
            },
            FormulaEntry {
                id: "ln",
                label: "ln",
                latex: r"\ln x",
            },
            FormulaEntry {
                id: "exp",
                label: "exp",
                latex: r"\exp x",
            },
            FormulaEntry {
                id: "hat",
                label: "hat",
                latex: r"\hat{x}",
            },
            FormulaEntry {
                id: "bar",
                label: "bar",
                latex: r"\bar{x}",
            },
            FormulaEntry {
                id: "vec",
                label: "vec",
                latex: r"\vec{x}",
            },
            FormulaEntry {
                id: "tilde",
                label: "tilde",
                latex: r"\tilde{x}",
            },
            FormulaEntry {
                id: "dot",
                label: "dot",
                latex: r"\dot{x}",
            },
            FormulaEntry {
                id: "ddot",
                label: "ddot",
                latex: r"\ddot{x}",
            },
            FormulaEntry {
                id: "overline",
                label: "上划线",
                latex: r"\overline{x}",
            },
            FormulaEntry {
                id: "underline",
                label: "下划线",
                latex: r"\underline{x}",
            },
            FormulaEntry {
                id: "overbrace",
                label: "上花括号",
                latex: r"\overbrace{x}^{n}",
            },
            FormulaEntry {
                id: "underbrace",
                label: "下花括号",
                latex: r"\underbrace{x}_{n}",
            },
        ],
    },
    FormulaCategory {
        name: "矩阵与分段",
        entries: &[
            FormulaEntry {
                id: "pmatrix",
                label: "pmatrix",
                latex: r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}",
            },
            FormulaEntry {
                id: "bmatrix",
                label: "bmatrix",
                latex: r"\begin{bmatrix} a & b \\ c & d \end{bmatrix}",
            },
            FormulaEntry {
                id: "vmatrix",
                label: "vmatrix",
                latex: r"\begin{vmatrix} a & b \\ c & d \end{vmatrix}",
            },
            FormulaEntry {
                id: "matrix",
                label: "matrix",
                latex: r"\begin{matrix} a & b \\ c & d \end{matrix}",
            },
            FormulaEntry {
                id: "cases",
                label: "分段",
                latex: r"\begin{cases} x & x>0 \\ -x & x\le 0 \end{cases}",
            },
            FormulaEntry {
                id: "aligned",
                label: "对齐",
                latex: r"\begin{aligned} a &= b \\ c &= d \end{aligned}",
            },
        ],
    },
    FormulaCategory {
        name: "大学数学常用",
        entries: &[
            FormulaEntry {
                id: "sin_limit",
                label: "重要极限",
                latex: r"\lim_{x\to 0}\frac{\sin x}{x}=1",
            },
            FormulaEntry {
                id: "gaussian_integral",
                label: "高斯积分",
                latex: r"\int_{0}^{\infty} e^{-x^{2}}\,\mathrm{d}x=\frac{\sqrt{\pi}}{2}",
            },
            FormulaEntry {
                id: "taylor_series",
                label: "泰勒展开",
                latex: r"f(x)=\sum_{n=0}^{\infty}\frac{f^{(n)}(a)}{n!}(x-a)^{n}",
            },
            FormulaEntry {
                id: "eigenvalue",
                label: "特征值方程",
                latex: r"A\mathbf{x}=\lambda\mathbf{x}",
            },
            FormulaEntry {
                id: "determinant",
                label: "特征行列式",
                latex: r"\det(A-\lambda I)=0",
            },
            FormulaEntry {
                id: "bayes",
                label: "贝叶斯公式",
                latex: r"\mathbb{P}(A\mid B)=\frac{\mathbb{P}(B\mid A)\mathbb{P}(A)}{\mathbb{P}(B)}",
            },
            FormulaEntry {
                id: "normal_distribution",
                label: "正态分布",
                latex: r"X\sim\mathcal{N}(\mu,\sigma^{2})",
            },
            FormulaEntry {
                id: "binomial_distribution",
                label: "二项分布",
                latex: r"\binom{n}{k}p^{k}(1-p)^{n-k}",
            },
            FormulaEntry {
                id: "euler_formula",
                label: "欧拉公式",
                latex: r"e^{i\theta}=\cos\theta+i\sin\theta",
            },
            FormulaEntry {
                id: "logic_statement",
                label: "全称命题",
                latex: r"\forall x\in\mathbb{R},\ \exists y:\ y>x",
            },
            FormulaEntry {
                id: "gradient",
                label: "梯度",
                latex: r"\nabla f",
            },
            FormulaEntry {
                id: "optimization",
                label: "最优化",
                latex: r"\arg\min_{x\in\mathbb{R}^{n}} f(x)",
            },
            FormulaEntry {
                id: "cauchy_schwarz",
                label: "柯西不等式",
                latex: r"\left|\langle u,v\rangle\right|\le\left\Vert u\right\Vert\left\Vert v\right\Vert",
            },
        ],
    },
];

/// Default favorites shown on the toolbar on first launch.
pub const DEFAULT_FAVORITE_IDS: &[&str] = &[
    "frac", "sqrt", "sup", "sub", "sum", "int", "lim", "bmatrix", "alpha", "cases",
];

/// Category whose entries form the one-click Greek palette on the toolbar.
pub const GREEK_CATEGORY_NAME: &str = "希腊字母";

/// Greek letters offered on the toolbar, in catalog order.
///
/// Reusing the category keeps labels and LaTeX in sync with the formula menu
/// instead of duplicating the table in the UI layer.
#[must_use]
pub fn greek_palette() -> &'static [FormulaEntry] {
    FORMULA_CATEGORIES
        .iter()
        .find(|category| category.name == GREEK_CATEGORY_NAME)
        .map_or(&[], |category| category.entries)
}

/// Returns `true` when `id` belongs to the Greek palette of the first toolbar row.
///
/// The formula row filters these out so a Greek letter never appears twice.
#[must_use]
pub fn is_greek(id: &str) -> bool {
    greek_palette().iter().any(|entry| entry.id == id)
}

/// Resolves a persisted identifier to its catalog entry.
#[must_use]
pub fn find(id: &str) -> Option<&'static FormulaEntry> {
    FORMULA_CATEGORIES
        .iter()
        .flat_map(|category| category.entries)
        .find(|entry| entry.id == id)
}

/// Default favorites as owned strings, ordered by catalog position.
#[must_use]
pub fn default_favorites() -> Vec<String> {
    let ids: Vec<String> = DEFAULT_FAVORITE_IDS
        .iter()
        .map(|id| (*id).to_owned())
        .collect();
    sanitize(&ids)
}

/// Drops unknown identifiers and duplicates while preserving catalog order.
#[must_use]
pub fn sanitize(ids: &[String]) -> Vec<String> {
    let mut kept: Vec<String> = Vec::new();
    for category in FORMULA_CATEGORIES {
        for entry in category.entries {
            if ids.iter().any(|id| id == entry.id) {
                kept.push(entry.id.to_owned());
            }
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_unique() {
        let mut seen = Vec::new();
        for category in FORMULA_CATEGORIES {
            for entry in category.entries {
                assert!(!seen.contains(&entry.id), "duplicate id: {}", entry.id);
                seen.push(entry.id);
            }
        }
    }

    #[test]
    fn default_favorites_are_resolvable() {
        for id in DEFAULT_FAVORITE_IDS {
            assert!(find(id).is_some(), "missing default favorite: {id}");
        }
    }

    #[test]
    fn sanitize_drops_unknown_and_duplicates() {
        let ids = vec![
            "sqrt".to_owned(),
            "missing".to_owned(),
            "sqrt".to_owned(),
            "frac".to_owned(),
        ];
        assert_eq!(sanitize(&ids), vec!["frac".to_owned(), "sqrt".to_owned()]);
    }
}
