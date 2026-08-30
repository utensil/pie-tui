//! Terminal-friendly rendering for a deliberately bounded LaTeX math subset.
//!
//! This is a clean-room parser and renderer, specified by black-box vectors
//! harvested from `@earendil-works/pi-tui@0.84.1`. It is pure: no terminal,
//! filesystem, clock, or sibling-crate dependency enters this module.

use crate::text::visible_width;
use regex::Regex;
use std::sync::OnceLock;

// JavaScript's parser advances one UTF-16 code unit at a time, including
// through valid surrogate pairs. Rust `char` cannot represent lone units, so
// the parser maps each surrogate to a collision-free supplementary PUA marker.
// Actual supplementary input reaches us as two UTF-16 units and is therefore
// mapped to two markers; no source unit can directly produce either marker.
const HIGH_SURROGATE_MARKER_START: u32 = 0xf0000;
const LOW_SURROGATE_MARKER_START: u32 = 0xf0400;

// U+10FFFF cannot collide with source: a literal U+10FFFF arrives as a mapped
// surrogate pair. It is reserved solely for matrix padding that survives the
// reference's trailing-space trim.
const PRESERVED_SPACE: char = '\u{10ffff}';

/// Options shared by [`render_latex_utf16`] and [`render_latex`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderLatexOptions {
    /// Stack fractions and large-operator limits vertically.
    pub display: bool,
}

/// Render a supported LaTeX expression through a strict Rust [`String`] boundary.
///
/// This scalar-friendly convenience wrapper encodes `source` as UTF-16, calls
/// [`render_latex_utf16`], and strictly decodes the exact output. Because the
/// reference parses JavaScript strings one UTF-16 code unit at a time, even
/// valid scalar input can produce unpaired output units; this wrapper returns
/// `None` when that exact result cannot be represented by [`String`]. It also
/// returns `None` for malformed input and unsupported commands. Call
/// [`render_latex_utf16`] when full JavaScript-string fidelity is required.
///
/// ```
/// use pie_core::latex::{RenderLatexOptions, render_latex};
///
/// let options = RenderLatexOptions::default();
/// assert_eq!(render_latex(r"\sqrt x", options), Some("√x".to_string()));
/// assert_eq!(render_latex(r"\sqrt 😀", options), None);
/// assert_eq!(render_latex(r"\hat 😀", options), None);
/// ```
pub fn render_latex(source: &str, options: RenderLatexOptions) -> Option<String> {
    String::from_utf16(&render_latex_utf16(
        &source.encode_utf16().collect::<Vec<_>>(),
        options,
    )?)
    .ok()
}

/// Render the canonical JavaScript-compatible boundary as raw UTF-16 units.
///
/// Unlike [`render_latex`], this exact core can preserve and return lone
/// surrogates. It is pure and never performs lossy UTF-16 conversion.
pub fn render_latex_utf16(source: &[u16], options: RenderLatexOptions) -> Option<Vec<u16>> {
    let internal = encode_units(source);
    debug_assert_eq!(js_unit_len(&internal), source.len());
    let mut parser = Parser::new(&internal);
    let expression = parser.parse_all().ok()?;
    let block = render_expression(&expression, options.display, false);
    let rendered = block
        .lines
        .join("\n")
        .trim_end()
        .replace(PRESERVED_SPACE, " ");
    Some(decode_units(&rendered))
}

fn encode_units(source: &[u16]) -> String {
    source
        .iter()
        .map(|unit| match *unit {
            0xd800..=0xdbff => {
                char::from_u32(HIGH_SURROGATE_MARKER_START + u32::from(*unit - 0xd800))
                    .expect("high-surrogate marker is scalar")
            }
            0xdc00..=0xdfff => {
                char::from_u32(LOW_SURROGATE_MARKER_START + u32::from(*unit - 0xdc00))
                    .expect("low-surrogate marker is scalar")
            }
            _ => char::from_u32(u32::from(*unit)).expect("non-surrogate UTF-16 unit is scalar"),
        })
        .collect()
}

fn decode_units(internal: &str) -> Vec<u16> {
    let mut units = Vec::with_capacity(internal.len());
    for ch in internal.chars() {
        let scalar = u32::from(ch);
        if ch == PRESERVED_SPACE {
            units.push(u16::from(b' '));
        } else if (HIGH_SURROGATE_MARKER_START..HIGH_SURROGATE_MARKER_START + 0x400)
            .contains(&scalar)
        {
            units.push((0xd800 + scalar - HIGH_SURROGATE_MARKER_START) as u16);
        } else if (LOW_SURROGATE_MARKER_START..LOW_SURROGATE_MARKER_START + 0x400).contains(&scalar)
        {
            units.push((0xdc00 + scalar - LOW_SURROGATE_MARKER_START) as u16);
        } else {
            units.extend(ch.encode_utf16(&mut [0; 2]).iter().copied());
        }
    }
    units
}

fn js_unit_len(internal: &str) -> usize {
    decode_units(internal).len()
}

fn js_string_element_count(internal: &str) -> usize {
    char::decode_utf16(decode_units(internal)).count()
}

fn is_reference_simple_text(internal: &str) -> bool {
    let mut saw_value = false;
    for value in char::decode_utf16(decode_units(internal)) {
        let Ok(ch) = value else {
            return false;
        };
        saw_value = true;
        if !ch.is_alphanumeric() {
            return false;
        }
    }
    saw_value
}

fn is_letter_number_or_dot_sequence(internal: &str) -> bool {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    matches_unit_sequence(
        internal,
        PATTERN.get_or_init(|| {
            Regex::new(r"^[\p{L}\p{N}.]+$").expect("letter/number/dot pattern is valid")
        }),
    )
}

fn is_number_or_dot_sequence(internal: &str) -> bool {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    matches_unit_sequence(
        internal,
        PATTERN.get_or_init(|| Regex::new(r"^[\p{N}.]+$").expect("number/dot pattern is valid")),
    )
}

fn matches_unit_sequence(internal: &str, pattern: &Regex) -> bool {
    String::from_utf16(&decode_units(internal))
        .ok()
        .is_some_and(|scalar| pattern.is_match(&scalar))
}

fn visible_width_internal(internal: &str) -> usize {
    let mut scalar = String::new();
    for ch in char::decode_utf16(decode_units(internal)).flatten() {
        scalar.push(ch);
    }
    visible_width(&scalar)
}

#[derive(Debug, Clone)]
struct Expression(Vec<Node>);

#[derive(Debug, Clone)]
enum Node {
    Text(String),
    Space,
    LineBreak,
    SpacedSymbol(String),
    Group(Expression),
    Styled {
        kind: StyleKind,
        body: Expression,
    },
    Fraction {
        numerator: Expression,
        denominator: Expression,
    },
    Root {
        degree: Option<String>,
        body: Expression,
    },
    Matrix {
        environment: String,
        rows: Vec<Vec<Expression>>,
    },
    Operator(String),
    Accent {
        name: &'static str,
        body: Expression,
        suffix: &'static str,
    },
    NamedFallback {
        name: String,
        body: Expression,
    },
    Scripted {
        base: Box<Node>,
        scripts: Vec<(char, Expression)>,
    },
}

#[derive(Debug, Clone, Copy)]
enum StyleKind {
    Plain,
    Blackboard,
}

struct Parser {
    chars: Vec<char>,
    position: usize,
}

impl Parser {
    fn new(source: &str) -> Self {
        Self {
            chars: source.chars().collect(),
            position: 0,
        }
    }

    fn parse_all(&mut self) -> Result<Expression, ()> {
        let expression = self.parse_expression(false)?;
        if self.position == self.chars.len() {
            Ok(expression)
        } else {
            Err(())
        }
    }

    fn parse_expression(&mut self, stop_at_brace: bool) -> Result<Expression, ()> {
        let mut nodes = Vec::new();
        while let Some(ch) = self.peek() {
            if ch == '}' {
                if stop_at_brace {
                    self.position += 1;
                    return Ok(Expression(nodes));
                }
                return Err(());
            }
            if ch.is_whitespace() {
                self.consume_whitespace();
                nodes.push(Node::Space);
                continue;
            }
            if matches!(ch, '_' | '^') {
                return Err(());
            }

            let mut node = self.parse_atom()?;
            let mut scripts = Vec::new();
            while let Some(marker @ ('_' | '^')) = self.peek() {
                if matches!(node, Node::Operator(_))
                    && scripts
                        .iter()
                        .any(|(existing_marker, _)| *existing_marker == marker)
                {
                    return Err(());
                }
                self.position += 1;
                let argument = self.parse_script_argument()?;
                scripts.push((marker, argument));
            }
            if !scripts.is_empty() {
                node = Node::Scripted {
                    base: Box::new(node),
                    scripts,
                };
            }
            nodes.push(node);
        }
        if stop_at_brace {
            Err(())
        } else {
            Ok(Expression(nodes))
        }
    }

    fn parse_atom(&mut self) -> Result<Node, ()> {
        match self.next().ok_or(())? {
            '{' => Ok(Node::Group(self.parse_expression(true)?)),
            '\\' => self.parse_command(),
            ch @ ('=' | '<' | '>') => Ok(Node::SpacedSymbol(ch.to_string())),
            '~' => Ok(Node::Space),
            ch => Ok(Node::Text(ch.to_string())),
        }
    }

    fn parse_script_argument(&mut self) -> Result<Expression, ()> {
        let Some(ch) = self.peek() else {
            return Err(());
        };
        if ch == '{' {
            self.position += 1;
            return self.parse_expression(true);
        }
        if ch.is_whitespace() || matches!(ch, '}' | '_' | '^') {
            return Err(());
        }
        Ok(Expression(vec![self.parse_atom()?]))
    }

    fn parse_command(&mut self) -> Result<Node, ()> {
        let Some(first) = self.peek() else {
            return Err(());
        };
        if !first.is_ascii_alphabetic() {
            self.position += 1;
            return match first {
                ',' | ':' | ';' | ' ' => Ok(Node::Space),
                '!' => Ok(Node::Group(Expression(Vec::new()))),
                '{' | '}' | '%' | '_' | '#' | '&' | '$' => Ok(Node::Text(first.to_string())),
                '\\' => Ok(Node::LineBreak),
                _ => Err(()),
            };
        }

        let start = self.position;
        while self.peek().is_some_and(|ch| ch.is_ascii_alphabetic()) {
            self.position += 1;
        }
        let command: String = self.chars[start..self.position].iter().collect();

        if let Some(symbol) = ordinary_command(&command) {
            return Ok(Node::Text(symbol.to_string()));
        }
        if let Some(symbol) = spaced_command(&command) {
            return Ok(Node::SpacedSymbol(symbol.to_string()));
        }
        if matches!(command.as_str(), "quad" | "qquad" | "enspace") {
            return Ok(Node::Space);
        }
        if let Some(name) = named_function(&command) {
            return Ok(Node::Text(name.to_string()));
        }
        if matches!(
            command.as_str(),
            "sum" | "prod" | "int" | "lim" | "min" | "max"
        ) {
            return Ok(Node::Operator(command));
        }

        match command.as_str() {
            "frac" | "dfrac" | "tfrac" => {
                let numerator = self.parse_argument()?;
                let denominator = self.parse_argument()?;
                Ok(Node::Fraction {
                    numerator,
                    denominator,
                })
            }
            "sqrt" => {
                let degree = self.parse_optional_bracket_text()?;
                let body = self.parse_argument()?;
                Ok(Node::Root { degree, body })
            }
            "text" | "textrm" | "mathrm" | "mathbf" | "mathit" | "mathsf" | "mathtt" => {
                Ok(Node::Styled {
                    kind: StyleKind::Plain,
                    body: self.parse_required_group()?,
                })
            }
            "mathbb" => Ok(Node::Styled {
                kind: StyleKind::Blackboard,
                body: self.parse_required_group()?,
            }),
            "vec" => Ok(Node::Accent {
                name: "vec",
                body: self.parse_argument()?,
                suffix: "\u{20d7}",
            }),
            "hat" => Ok(Node::Accent {
                name: "hat",
                body: self.parse_argument()?,
                suffix: "\u{302}",
            }),
            "bar" => Ok(Node::Accent {
                name: "bar",
                body: self.parse_argument()?,
                suffix: "\u{305}",
            }),
            "overline" | "underline" => Ok(Node::NamedFallback {
                name: command,
                body: self.parse_required_group()?,
            }),
            "left" | "middle" | "right" => self.parse_delimiter(),
            "begin" => self.parse_environment(),
            _ => Err(()),
        }
    }

    fn parse_delimiter(&mut self) -> Result<Node, ()> {
        while self.peek().is_some_and(char::is_whitespace) {
            self.position += 1;
        }
        let delimiter = match self.next().ok_or(())? {
            '\\' => {
                let Some(first) = self.next() else {
                    return Err(());
                };
                if first.is_ascii_alphabetic() {
                    let start = self.position - 1;
                    while self.peek().is_some_and(|ch| ch.is_ascii_alphabetic()) {
                        self.position += 1;
                    }
                    let name: String = self.chars[start..self.position].iter().collect();
                    match name.as_str() {
                        "langle" => "⟨",
                        "rangle" => "⟩",
                        "lbrace" => "{",
                        "rbrace" => "}",
                        "vert" => "|",
                        "Vert" => "‖",
                        _ => return Err(()),
                    }
                } else {
                    match first {
                        '{' => "{",
                        '}' => "}",
                        '|' => "‖",
                        other => return Ok(Node::Text(other.to_string())),
                    }
                }
            }
            '.' => "",
            '(' => "(",
            ')' => ")",
            '[' => "[",
            ']' => "]",
            '|' => "|",
            other => return Ok(Node::Text(other.to_string())),
        };
        Ok(Node::Text(delimiter.to_string()))
    }

    fn parse_argument(&mut self) -> Result<Expression, ()> {
        self.consume_whitespace();
        if self.peek() == Some('{') {
            self.position += 1;
            return self.parse_expression(true);
        }
        let node = self.parse_atom()?;
        Ok(Expression(vec![node]))
    }

    fn parse_environment(&mut self) -> Result<Node, ()> {
        let environment = self.parse_required_group_raw()?;
        if !matches!(
            environment.as_str(),
            "matrix" | "pmatrix" | "bmatrix" | "vmatrix" | "Vmatrix" | "cases"
        ) {
            return Err(());
        }
        let marker: Vec<char> = format!("\\end{{{environment}}}").chars().collect();
        let end = self.find_sequence(&marker).ok_or(())?;
        let body: String = self.chars[self.position..end].iter().collect();
        self.position = end + marker.len();
        let raw_rows = split_matrix(&body);
        if raw_rows.is_empty() {
            return Err(());
        }
        let mut rows = Vec::with_capacity(raw_rows.len());
        for row in raw_rows {
            let mut cells = Vec::with_capacity(row.len());
            for cell in row {
                cells.push(Parser::new(cell.trim()).parse_all()?);
            }
            rows.push(cells);
        }
        Ok(Node::Matrix { environment, rows })
    }

    fn parse_required_group(&mut self) -> Result<Expression, ()> {
        if self.next() != Some('{') {
            return Err(());
        }
        self.parse_expression(true)
    }

    fn parse_required_group_raw(&mut self) -> Result<String, ()> {
        if self.next() != Some('{') {
            return Err(());
        }
        let start = self.position;
        while let Some(ch) = self.next() {
            if ch == '}' {
                return Ok(self.chars[start..self.position - 1].iter().collect());
            }
            if ch == '{' {
                return Err(());
            }
        }
        Err(())
    }

    fn parse_optional_bracket_text(&mut self) -> Result<Option<String>, ()> {
        if self.peek() != Some('[') {
            return Ok(None);
        }
        self.position += 1;
        let start = self.position;
        while let Some(ch) = self.next() {
            if ch == ']' {
                return Ok(Some(self.chars[start..self.position - 1].iter().collect()));
            }
        }
        Err(())
    }

    fn find_sequence(&self, needle: &[char]) -> Option<usize> {
        self.chars[self.position..]
            .windows(needle.len())
            .position(|window| window == needle)
            .map(|offset| self.position + offset)
    }

    fn consume_whitespace(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.position += 1;
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.position).copied()
    }

    fn next(&mut self) -> Option<char> {
        let value = self.peek()?;
        self.position += 1;
        Some(value)
    }
}

fn split_matrix(source: &str) -> Vec<Vec<String>> {
    let chars: Vec<char> = source.chars().collect();
    let mut rows = vec![vec![String::new()]];
    let mut depth = 0usize;
    let mut index = 0usize;
    while index < chars.len() {
        match chars[index] {
            '{' => {
                depth += 1;
                rows.last_mut().unwrap().last_mut().unwrap().push('{');
            }
            '}' => {
                depth = depth.saturating_sub(1);
                rows.last_mut().unwrap().last_mut().unwrap().push('}');
            }
            '&' if depth == 0 => rows.last_mut().unwrap().push(String::new()),
            '\\' if depth == 0 && chars.get(index + 1) == Some(&'\\') => {
                rows.push(vec![String::new()]);
                index += 1;
            }
            ch => rows.last_mut().unwrap().last_mut().unwrap().push(ch),
        }
        index += 1;
    }
    rows
}

fn ordinary_command(command: &str) -> Option<&'static str> {
    Some(match command {
        "alpha" => "α",
        "beta" => "β",
        "gamma" => "γ",
        "delta" => "δ",
        "epsilon" | "varepsilon" => "ε",
        "zeta" => "ζ",
        "eta" => "η",
        "theta" => "θ",
        "vartheta" => "ϑ",
        "iota" => "ι",
        "kappa" => "κ",
        "lambda" => "λ",
        "mu" => "μ",
        "nu" => "ν",
        "xi" => "ξ",
        "pi" => "π",
        "varpi" => "ϖ",
        "rho" => "ρ",
        "varrho" => "ϱ",
        "sigma" => "σ",
        "varsigma" => "ς",
        "tau" => "τ",
        "upsilon" => "υ",
        "phi" | "varphi" => "φ",
        "chi" => "χ",
        "psi" => "ψ",
        "omega" => "ω",
        "Gamma" => "Γ",
        "Delta" => "Δ",
        "Theta" => "Θ",
        "Lambda" => "Λ",
        "Xi" => "Ξ",
        "Pi" => "Π",
        "Sigma" => "Σ",
        "Upsilon" => "Υ",
        "Phi" => "Φ",
        "Psi" => "Ψ",
        "Omega" => "Ω",
        "infty" => "∞",
        "partial" => "∂",
        "nabla" => "∇",
        "ell" => "ℓ",
        "hbar" => "ℏ",
        "emptyset" | "varnothing" => "∅",
        "ldots" | "dots" => "…",
        "cdots" => "⋯",
        "vdots" => "⋮",
        "ddots" => "⋱",
        "forall" => "∀",
        "exists" => "∃",
        "pm" => "±",
        "div" => "÷",
        _ => return None,
    })
}

fn spaced_command(command: &str) -> Option<&'static str> {
    Some(match command {
        "to" | "rightarrow" => "→",
        "longrightarrow" => "→",
        "leftarrow" => "←",
        "leftrightarrow" => "↔",
        "Rightarrow" => "⇒",
        "Leftarrow" => "⇐",
        "Leftrightarrow" => "⇔",
        "mapsto" => "↦",
        "uparrow" => "↑",
        "downarrow" => "↓",
        "in" => "∈",
        "notin" => "∉",
        "ni" => "∋",
        "le" | "leq" => "≤",
        "ge" | "geq" => "≥",
        "neq" | "ne" => "≠",
        "approx" => "≈",
        "equiv" => "≡",
        "sim" => "∼",
        "simeq" => "≃",
        "cong" => "≅",
        "ll" => "≪",
        "gg" => "≫",
        "subset" => "⊂",
        "supset" => "⊃",
        "subseteq" => "⊆",
        "supseteq" => "⊇",
        "cup" => "∪",
        "cap" => "∩",
        "land" | "wedge" => "∧",
        "lor" | "vee" => "∨",
        "times" => "×",
        "cdot" => "·",
        "mp" => "∓",
        "ast" => "∗",
        "star" => "⋆",
        "circ" => "∘",
        "bullet" => "•",
        "oplus" => "⊕",
        "otimes" => "⊗",
        "propto" => "∝",
        _ => return None,
    })
}

fn named_function(command: &str) -> Option<&'static str> {
    Some(match command {
        "sin" => "sin",
        "cos" => "cos",
        "tan" => "tan",
        "cot" => "cot",
        "sec" => "sec",
        "csc" => "csc",
        "arcsin" => "arcsin",
        "arccos" => "arccos",
        "arctan" => "arctan",
        "sinh" => "sinh",
        "cosh" => "cosh",
        "tanh" => "tanh",
        "log" => "log",
        "ln" => "ln",
        "exp" => "exp",
        "det" => "det",
        "gcd" => "gcd",
        _ => return None,
    })
}

#[derive(Debug, Clone)]
struct Block {
    lines: Vec<String>,
    baseline: usize,
}

impl Block {
    fn text(text: impl Into<String>) -> Self {
        Self {
            lines: vec![text.into()],
            baseline: 0,
        }
    }

    fn width(&self) -> usize {
        self.lines
            .iter()
            .map(|line| visible_width_internal(line))
            .max()
            .unwrap_or(0)
    }

    fn is_empty(&self) -> bool {
        self.lines.iter().all(String::is_empty)
    }
}

fn render_expression(expression: &Expression, display: bool, script_context: bool) -> Block {
    if expression
        .0
        .iter()
        .any(|node| matches!(node, Node::LineBreak))
    {
        let mut lines = Vec::new();
        let mut segment = Vec::new();
        for node in &expression.0 {
            if matches!(node, Node::LineBreak) {
                lines.extend(
                    render_expression(
                        &Expression(std::mem::take(&mut segment)),
                        display,
                        script_context,
                    )
                    .lines,
                );
            } else {
                segment.push(node.clone());
            }
        }
        lines.extend(render_expression(&Expression(segment), display, script_context).lines);
        return Block { lines, baseline: 0 };
    }

    let mut blocks = Vec::new();
    let mut pending_space = false;
    for node in &expression.0 {
        if matches!(node, Node::Space) {
            if !script_context {
                pending_space = !blocks.is_empty();
            }
            continue;
        }
        if let Node::SpacedSymbol(symbol) = node {
            if script_context {
                blocks.push(Block::text(symbol));
            } else {
                if !blocks.is_empty() {
                    push_space(&mut blocks);
                }
                blocks.push(Block::text(symbol));
                pending_space = true;
            }
            continue;
        }

        let block = render_node(node, display, script_context);
        if block.is_empty() {
            continue;
        }
        let stacked_fraction = display && is_stacked_fraction(node);
        let has_intrinsic_leading_space = block
            .lines
            .get(block.baseline)
            .is_some_and(|line| line.starts_with(' '));
        if !script_context
            && ((pending_space && !has_intrinsic_leading_space)
                || (stacked_fraction && !blocks.is_empty()))
        {
            push_space(&mut blocks);
        }
        blocks.push(block);
        pending_space = !script_context
            && (stacked_fraction || (display && is_large_operator_with_limits(node)));
    }
    compose_blocks(&blocks)
}

fn is_large_operator_with_limits(node: &Node) -> bool {
    matches!(
        node,
        Node::Scripted { base, scripts }
            if !scripts.is_empty()
                && matches!(base.as_ref(), Node::Operator(operator) if !matches!(operator.as_str(), "lim" | "min" | "max"))
    )
}

fn push_space(blocks: &mut Vec<Block>) {
    if blocks
        .last()
        .is_some_and(|block| block.lines.len() == 1 && block.lines[0] == " ")
    {
        return;
    }
    blocks.push(Block::text(" "));
}

fn is_stacked_fraction(node: &Node) -> bool {
    match node {
        Node::Fraction { .. } => true,
        Node::Scripted { base, .. } => matches!(base.as_ref(), Node::Fraction { .. }),
        _ => false,
    }
}

fn render_node(node: &Node, display: bool, script_context: bool) -> Block {
    match node {
        Node::Text(text) => Block::text(text),
        Node::Space => Block::text(" "),
        Node::LineBreak => Block::text("\n"),
        Node::SpacedSymbol(symbol) => Block::text(symbol),
        Node::Group(body) => render_expression(body, display, script_context),
        Node::Styled { kind, body } => {
            let mut block = render_expression(body, display, script_context);
            if matches!(kind, StyleKind::Blackboard) {
                for line in &mut block.lines {
                    *line = line.chars().map(blackboard_char).collect();
                }
            }
            block
        }
        Node::Fraction {
            numerator,
            denominator,
        } => render_fraction(numerator, denominator, display),
        Node::Root { degree, body } => render_root(degree.as_deref(), body),
        Node::Matrix { environment, rows } => render_matrix(environment, rows),
        Node::Operator(operator) => Block::text(operator_symbol(operator)),
        Node::Accent { name, body, suffix } => {
            let rendered = render_inline_string(body, false);
            if js_string_element_count(&rendered) == 1 {
                Block::text(format!("{rendered}{suffix}"))
            } else {
                Block::text(format!("{name}({rendered})"))
            }
        }
        Node::NamedFallback { name, body } => {
            let rendered = render_inline_string(body, false);
            Block::text(format!("{name}({rendered})"))
        }
        Node::Scripted { base, scripts } => render_scripted(base, scripts, display),
    }
}

fn render_fraction(numerator: &Expression, denominator: &Expression, display: bool) -> Block {
    let numerator_text = render_inline_string(numerator, false);
    let denominator_text = render_inline_string(denominator, false);
    if !display {
        let numerator =
            parenthesize_fraction_part(&numerator_text, expression_is_fraction(numerator), false);
        let denominator = parenthesize_fraction_part(
            &denominator_text,
            expression_is_fraction(denominator),
            true,
        );
        return Block::text(format!("{numerator}/{denominator}"));
    }
    let width = visible_width_internal(&numerator_text)
        .max(visible_width_internal(&denominator_text))
        .max(1);
    Block {
        lines: vec![
            center(&numerator_text, width),
            "─".repeat(width),
            center(&denominator_text, width),
        ],
        baseline: 1,
    }
}

fn expression_is_fraction(expression: &Expression) -> bool {
    match expression.0.as_slice() {
        [Node::Fraction { .. }] => true,
        [Node::Scripted { base, .. }] => matches!(base.as_ref(), Node::Fraction { .. }),
        _ => false,
    }
}

fn parenthesize_fraction_part(part: &str, nested_fraction: bool, denominator: bool) -> String {
    if part.is_empty() {
        "()".to_string()
    } else if nested_fraction
        || if denominator {
            !(is_number_or_dot_sequence(part) || js_string_element_count(part) == 1)
        } else {
            !is_letter_number_or_dot_sequence(part)
        }
    {
        format!("({part})")
    } else {
        part.to_string()
    }
}

fn render_root(degree: Option<&str>, body: &Expression) -> Block {
    let body = render_inline_string(body, false);
    let radical = match degree.map(str::trim) {
        None | Some("") | Some("2") => "√",
        Some("3") => "∛",
        Some("4") => "∜",
        Some(other) => {
            let degree = format_superscript(other);
            return if !is_letter_number_or_dot_sequence(&body) {
                Block::text(format!("{degree}√({body})"))
            } else {
                Block::text(format!("{degree}√{body}"))
            };
        }
    };
    if body.is_empty() {
        return Block::text(format!("{radical}()"));
    }
    let needs_parentheses = !is_letter_number_or_dot_sequence(&body);
    if needs_parentheses {
        Block::text(format!("{radical}({body})"))
    } else {
        Block::text(format!("{radical}{body}"))
    }
}

fn render_scripted(base: &Node, scripts: &[(char, Expression)], display: bool) -> Block {
    if let Node::Operator(operator) = base {
        let subscript = scripts
            .iter()
            .rev()
            .find(|(marker, _)| *marker == '_')
            .map(|(_, expression)| expression);
        let superscript = scripts
            .iter()
            .rev()
            .find(|(marker, _)| *marker == '^')
            .map(|(_, expression)| expression);
        return render_operator(operator, subscript, superscript, display);
    }

    let mut block = render_node(base, display, false);
    let mut suffix = String::new();
    for (marker, expression) in scripts {
        let rendered = render_inline_string(expression, true);
        if rendered.is_empty() {
            continue;
        }
        if *marker == '_' {
            if expression_contains_script(expression) {
                suffix.push_str(&format!("_({rendered})"));
            } else {
                suffix.push_str(&format_subscript(&rendered));
            }
        } else {
            if expression_contains_script(expression) {
                suffix.push_str(&format!("^({rendered})"));
            } else {
                suffix.push_str(&format_superscript(&rendered));
            }
        }
    }
    if let Some(line) = block.lines.get_mut(block.baseline) {
        line.push_str(&suffix);
    }
    block
}

fn expression_contains_script(expression: &Expression) -> bool {
    expression.0.iter().any(node_contains_script)
}

fn node_contains_script(node: &Node) -> bool {
    match node {
        Node::Scripted { .. } => true,
        Node::Group(body)
        | Node::Styled { body, .. }
        | Node::Accent { body, .. }
        | Node::NamedFallback { body, .. } => expression_contains_script(body),
        Node::Fraction {
            numerator,
            denominator,
        } => expression_contains_script(numerator) || expression_contains_script(denominator),
        Node::Root { body, .. } => expression_contains_script(body),
        Node::Matrix { rows, .. } => rows.iter().flatten().any(expression_contains_script),
        Node::Text(_)
        | Node::Space
        | Node::LineBreak
        | Node::SpacedSymbol(_)
        | Node::Operator(_) => false,
    }
}

fn render_operator(
    operator: &str,
    subscript: Option<&Expression>,
    superscript: Option<&Expression>,
    display: bool,
) -> Block {
    let symbol = operator_symbol(operator);
    let subscript = subscript.map(|value| render_inline_string(value, true));
    let superscript = superscript.map(|value| render_inline_string(value, true));
    if !display {
        if operator == "lim" || operator == "min" || operator == "max" {
            let mut rendered = symbol.to_string();
            if let Some(subscript) = subscript {
                rendered.push('[');
                rendered.push_str(&subscript);
                rendered.push(']');
            }
            if let Some(superscript) = superscript {
                rendered.push_str(&format_superscript(&superscript));
            }
            return Block::text(rendered);
        }
        return Block::text(format!(
            "{symbol}{}",
            format_scripts(subscript.as_deref(), superscript.as_deref())
        ));
    }

    if operator == "lim" || operator == "min" || operator == "max" {
        let width = visible_width_internal(symbol).max(
            subscript
                .as_deref()
                .map(visible_width_internal)
                .unwrap_or_default(),
        );
        let mut lines = vec![center(symbol, width)];
        if let Some(subscript) = subscript {
            lines.push(center(&subscript, width));
        }
        return Block { lines, baseline: 0 };
    }

    let width = visible_width_internal(symbol)
        .max(
            subscript
                .as_deref()
                .map(visible_width_internal)
                .unwrap_or_default(),
        )
        .max(
            superscript
                .as_deref()
                .map(visible_width_internal)
                .unwrap_or_default(),
        );
    let mut lines = Vec::new();
    let baseline = usize::from(superscript.is_some());
    if let Some(superscript) = superscript {
        lines.push(center(&superscript, width));
    }
    lines.push(center(symbol, width));
    if let Some(subscript) = subscript {
        lines.push(center(&subscript, width));
    }
    Block { lines, baseline }
}

fn operator_symbol(operator: &str) -> &'static str {
    match operator {
        "sum" => "∑",
        "prod" => "∏",
        "int" => "∫",
        "lim" => "lim",
        "min" => "min",
        "max" => "max",
        _ => "",
    }
}

fn format_scripts(subscript: Option<&str>, superscript: Option<&str>) -> String {
    let mut result = String::new();
    if let Some(subscript) = subscript {
        result.push_str(&format_subscript(subscript));
    }
    if let Some(superscript) = superscript {
        result.push_str(&format_superscript(superscript));
    }
    result
}

fn format_superscript(value: &str) -> String {
    if value == "∞" {
        return format!("^{value}");
    }
    value
        .chars()
        .map(superscript_char)
        .collect::<Option<String>>()
        .unwrap_or_else(|| {
            if js_string_element_count(value) == 1 {
                format!("^{value}")
            } else {
                format!("^({value})")
            }
        })
}

fn format_subscript(value: &str) -> String {
    value
        .chars()
        .map(subscript_char)
        .collect::<Option<String>>()
        .unwrap_or_else(|| {
            if js_string_element_count(value) == 1 || is_reference_simple_text(value) {
                format!("_{value}")
            } else {
                format!("_({value})")
            }
        })
}

fn superscript_char(ch: char) -> Option<char> {
    Some(match ch {
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        '+' => '⁺',
        '-' => '⁻',
        '=' => '⁼',
        '(' => '⁽',
        ')' => '⁾',
        'a' => 'ᵃ',
        'b' => 'ᵇ',
        'c' => 'ᶜ',
        'd' => 'ᵈ',
        'e' => 'ᵉ',
        'f' => 'ᶠ',
        'g' => 'ᵍ',
        'h' => 'ʰ',
        'i' => 'ⁱ',
        'j' => 'ʲ',
        'k' => 'ᵏ',
        'l' => 'ˡ',
        'm' => 'ᵐ',
        'n' => 'ⁿ',
        'o' => 'ᵒ',
        'p' => 'ᵖ',
        'r' => 'ʳ',
        's' => 'ˢ',
        't' => 'ᵗ',
        'u' => 'ᵘ',
        'v' => 'ᵛ',
        'w' => 'ʷ',
        'x' => 'ˣ',
        'y' => 'ʸ',
        'z' => 'ᶻ',
        _ => return None,
    })
}

fn subscript_char(ch: char) -> Option<char> {
    Some(match ch {
        '0' => '₀',
        '1' => '₁',
        '2' => '₂',
        '3' => '₃',
        '4' => '₄',
        '5' => '₅',
        '6' => '₆',
        '7' => '₇',
        '8' => '₈',
        '9' => '₉',
        '+' => '₊',
        '-' => '₋',
        '=' => '₌',
        '(' => '₍',
        ')' => '₎',
        'a' => 'ₐ',
        'e' => 'ₑ',
        'h' => 'ₕ',
        'i' => 'ᵢ',
        'j' => 'ⱼ',
        'k' => 'ₖ',
        'l' => 'ₗ',
        'm' => 'ₘ',
        'n' => 'ₙ',
        'o' => 'ₒ',
        'p' => 'ₚ',
        'r' => 'ᵣ',
        's' => 'ₛ',
        't' => 'ₜ',
        'u' => 'ᵤ',
        'v' => 'ᵥ',
        'x' => 'ₓ',
        _ => return None,
    })
}

fn render_inline_string(expression: &Expression, script_context: bool) -> String {
    render_expression(expression, false, script_context)
        .lines
        .join(" ")
        .trim()
        .to_string()
}

fn render_matrix(environment: &str, rows: &[Vec<Expression>]) -> Block {
    if environment == "cases" {
        let mut lines = Vec::with_capacity(rows.len());
        for (row_index, row) in rows.iter().enumerate() {
            let (left, _) = matrix_delimiters(environment, row_index, rows.len());
            let value = row
                .first()
                .map(|cell| render_inline_string(cell, false))
                .unwrap_or_default();
            let condition = row
                .get(1)
                .map(|cell| render_inline_string(cell, false))
                .unwrap_or_default();
            if condition.is_empty() {
                lines.push(format!("{left} {value}"));
            } else {
                lines.push(format!("{left} {value} if {condition}"));
            }
        }
        return Block { lines, baseline: 0 };
    }

    let column_count = rows.iter().map(Vec::len).max().unwrap_or(0);
    let rendered_rows: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| render_inline_string(cell, false))
                .collect()
        })
        .collect();
    let mut column_widths = vec![0usize; column_count];
    for row in &rendered_rows {
        for (column, cell) in row.iter().enumerate() {
            column_widths[column] = column_widths[column].max(visible_width_internal(cell));
        }
    }

    let mut lines = Vec::with_capacity(rendered_rows.len());
    for (row_index, row) in rendered_rows.iter().enumerate() {
        let (left, right) = matrix_delimiters(environment, row_index, rendered_rows.len());
        let mut cells = Vec::with_capacity(column_count);
        for (column, width) in column_widths.iter().copied().enumerate() {
            let cell = row.get(column).map(String::as_str).unwrap_or("");
            if column + 1 == column_count && right.is_empty() {
                cells.push(pad_right_preserved(cell, width));
            } else {
                cells.push(pad_right(cell, width));
            }
        }
        let inner = cells.join(" │ ");
        if left.is_empty() {
            lines.push(inner);
        } else {
            lines.push(format!("{left} {inner} {right}"));
        }
    }
    Block { lines, baseline: 0 }
}

fn matrix_delimiters(environment: &str, row: usize, rows: usize) -> (&'static str, &'static str) {
    match environment {
        "pmatrix" => bracket_pair(row, rows, ("⎛", "⎞"), ("⎜", "⎟"), ("⎝", "⎠"), ("⎛", "⎞")),
        "bmatrix" => bracket_pair(row, rows, ("⎡", "⎤"), ("⎢", "⎥"), ("⎣", "⎦"), ("[", "]")),
        "vmatrix" => ("│", "│"),
        "Vmatrix" => ("‖", "‖"),
        "cases" => {
            if rows == 1 {
                ("{", "")
            } else if row == 0 {
                ("⎧", "")
            } else if row + 1 == rows {
                ("⎩", "")
            } else {
                ("⎨", "")
            }
        }
        _ => ("", ""),
    }
}

fn bracket_pair(
    row: usize,
    rows: usize,
    top: (&'static str, &'static str),
    middle: (&'static str, &'static str),
    bottom: (&'static str, &'static str),
    single: (&'static str, &'static str),
) -> (&'static str, &'static str) {
    if rows == 1 {
        single
    } else if row == 0 {
        top
    } else if row + 1 == rows {
        bottom
    } else {
        middle
    }
}

fn compose_blocks(blocks: &[Block]) -> Block {
    if blocks.is_empty() {
        return Block::text("");
    }
    let baseline = blocks.iter().map(|block| block.baseline).max().unwrap_or(0);
    let below = blocks
        .iter()
        .map(|block| block.lines.len().saturating_sub(block.baseline + 1))
        .max()
        .unwrap_or(0);
    let height = baseline + below + 1;
    let mut lines = Vec::with_capacity(height);
    for output_row in 0..height {
        let mut line = String::new();
        for block in blocks {
            let width = block.width();
            let local_row = output_row as isize + block.baseline as isize - baseline as isize;
            if local_row >= 0 && (local_row as usize) < block.lines.len() {
                line.push_str(&pad_right(&block.lines[local_row as usize], width));
            } else {
                line.push_str(&" ".repeat(width));
            }
        }
        lines.push(line.trim_end_matches(' ').to_string());
    }
    Block { lines, baseline }
}

fn center(text: &str, width: usize) -> String {
    let padding = width.saturating_sub(visible_width_internal(text));
    let left = padding / 2;
    let right = padding - left;
    format!("{}{}{}", " ".repeat(left), text, " ".repeat(right))
}

fn pad_right(text: &str, width: usize) -> String {
    format!(
        "{text}{}",
        " ".repeat(width.saturating_sub(visible_width_internal(text)))
    )
}

fn pad_right_preserved(text: &str, width: usize) -> String {
    format!(
        "{text}{}",
        PRESERVED_SPACE
            .to_string()
            .repeat(width.saturating_sub(visible_width_internal(text)))
    )
}

fn blackboard_char(ch: char) -> char {
    match ch {
        'C' => 'ℂ',
        'H' => 'ℍ',
        'N' => 'ℕ',
        'P' => 'ℙ',
        'Q' => 'ℚ',
        'R' => 'ℝ',
        'Z' => 'ℤ',
        _ => ch,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{PRESERVED_SPACE, decode_units, encode_units};

    #[test]
    fn internal_unit_encoding_is_a_bijection_for_every_u16() {
        let mut encodings = BTreeSet::new();
        for unit in 0..=u16::MAX {
            let encoded = encode_units(&[unit]);
            assert_eq!(encoded.chars().count(), 1, "unit {unit} encoding width");
            assert!(
                encodings.insert(encoded.chars().next().expect("one encoded scalar")),
                "unit {unit} must not collide"
            );
            assert_eq!(decode_units(&encoded), vec![unit], "unit {unit} roundtrip");
        }
        assert_eq!(encodings.len(), usize::from(u16::MAX) + 1);
        assert!(!encodings.contains(&PRESERVED_SPACE));

        for source in [
            vec![0xd800, 0xdc00],
            vec![0xdb80, 0xdc00],
            vec![0xdbff, 0xdfff],
            vec![0xd800, 0xd800, 0xdc00, 0xdc00],
            vec![0xe000, 0xd83d, 0xde00, 0xdbff, 0xdfff],
        ] {
            assert_eq!(decode_units(&encode_units(&source)), source);
        }
    }
}
