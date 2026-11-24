//! Small, dependency-free math expression evaluator for plotting `y = f(x)`.
//!
//! Supported syntax:
//! - numbers (decimal and scientific notation, e.g. `1e-3`),
//! - `x` and the constants `pi`, `e`, `tau`,
//! - `+ - * / ^` (`^` is right-associative, unary minus has lower precedence
//!   than `^`, so `-2^2 == -4`),
//! - parentheses, and function calls such as `sin(x)` or `pow(x, 2)`.
//!
//! `log` is the natural logarithm (an alias of `ln`); `log10` is base 10.
//!
//! Syntax errors are returned as strings with a 0-based character position.
//! Domain errors of the math functions (`sqrt(-1)`, `ln(0)`, ...) are *not*
//! errors: they produce `NaN`/infinity so a plotter can skip or clip them.
//! `evaluate` reports division by zero as an error; [`sample`] keeps such
//! singular points as infinities so that a curve still has all `count` points.

use std::f64::consts::{E, PI, TAU};

/// `(label, expression, xmin, xmax, ymin, ymax)` ready-made curves.
///
/// The ranges are chosen so each curve appears useful immediately; the user can
/// still edit everything before drawing.
pub const PRESETS: &[(&str, &str, f64, f64, f64, f64)] = &[
    // Basics
    ("sin(x)", "sin(x)", -10.0, 10.0, -2.0, 2.0),
    ("cos(x)", "cos(x)", -10.0, 10.0, -2.0, 2.0),
    ("tan(x)", "tan(x)", -6.4, 6.4, -6.0, 6.0),
    ("x^2", "x^2", -5.0, 5.0, -1.0, 10.0),
    ("x^3", "x^3", -4.0, 4.0, -20.0, 20.0),
    ("exp(x)", "exp(x)", -4.0, 4.0, -1.0, 20.0),
    ("ln(x)", "ln(max(x, 1e-12))", 0.1, 12.0, -3.0, 3.0),
    ("1/x", "1/x", -6.0, 6.0, -6.0, 6.0),
    ("sqrt(x)", "sqrt(max(x, 0))", 0.0, 10.0, -0.5, 4.0),
    ("abs(x)", "abs(x)", -6.0, 6.0, -0.5, 6.0),
    ("gaussian", "exp(-x^2/2)", -5.0, 5.0, -0.2, 1.2),
    ("sinc", "sin(x)/max(abs(x),1e-6)", -15.0, 15.0, -0.4, 1.2),
    ("damped sine", "exp(-x/5)*sin(x)", -2.0, 20.0, -1.2, 1.2),
    ("cos(x^2)", "cos(x^2)", -6.0, 6.0, -1.5, 1.5),
    // Oscilloscope / RF
    ("oscilloscope sine", "sin(2*pi*x)", 0.0, 2.0, -1.6, 1.6),
    (
        "AM envelope",
        "(1+0.6*sin(2*pi*x))*sin(2*pi*12*x)",
        0.0,
        2.0,
        -2.2,
        2.2,
    ),
    (
        "FM-ish",
        "sin(2*pi*4*x+2*sin(2*pi*0.5*x))",
        0.0,
        2.0,
        -1.6,
        1.6,
    ),
    ("square wave", "square(2*pi*x)", 0.0, 2.0, -1.8, 1.8),
    ("sawtooth", "saw(2*pi*x)", 0.0, 2.0, -1.6, 1.6),
    ("triangle wave", "tri(2*pi*x)", 0.0, 2.0, -1.6, 1.6),
    (
        "PWM 25%",
        "step(sin(2*pi*x)-cos(pi*0.25))*2-1",
        0.0,
        2.0,
        -1.8,
        1.8,
    ),
    ("Lissajous X", "sin(3*x+pi/2)", 0.0, 6.283, -1.6, 1.6),
    ("Lissajous Y (param)", "sin(2*x)", 0.0, 6.283, -1.6, 1.6),
    (
        "carrier + noise-like",
        "sin(2*pi*8*x)+0.25*sin(2*pi*37*x)",
        0.0,
        2.0,
        -1.8,
        1.8,
    ),
    (
        "RLC ringdown",
        "exp(-2*x)*cos(2*pi*6*x)",
        0.0,
        2.0,
        -1.2,
        1.2,
    ),
    ("low-pass step response", "1-exp(-5*x)", 0.0, 2.0, -0.2, 1.4),
    ("rectified sine", "abs(sin(2*pi*x))", 0.0, 2.0, -0.2, 1.4),
    (
        "RC charge/discharge",
        "step(1-mod(x,2))*(1-exp(-4*mod(x,2))) + (1-step(1-mod(x,2)))*exp(-4*(mod(x,2)-1))",
        0.0,
        4.0,
        -0.2,
        1.4,
    ),
];

const MAX_DEPTH: usize = 128;
const MAX_TOKENS: usize = 4096;

// ---------------------------------------------------------------------------
// Tokenizer
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum TokenKind {
    Number(f64),
    Ident(String),
    Plus,
    Minus,
    Mul,
    Div,
    Pow,
    LParen,
    RParen,
    Comma,
    Question,
    Colon,
}

#[derive(Clone, Debug)]
struct Token {
    kind: TokenKind,
    pos: usize,
}

impl Token {
    fn describe(&self) -> String {
        match &self.kind {
            TokenKind::Number(n) => format!("number '{}'", n),
            TokenKind::Ident(s) => format!("identifier '{}'", s),
            TokenKind::Plus => "'+'".to_owned(),
            TokenKind::Minus => "'-'".to_owned(),
            TokenKind::Mul => "'*'".to_owned(),
            TokenKind::Div => "'/'".to_owned(),
            TokenKind::Pow => "'^'".to_owned(),
            TokenKind::LParen => "'('".to_owned(),
            TokenKind::RParen => "')'".to_owned(),
            TokenKind::Comma => "','".to_owned(),
            TokenKind::Question => "'?'".to_owned(),
            TokenKind::Colon => "':'".to_owned(),
        }
    }
}

fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = input.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }

        let token_pos = i;
        let kind = match c {
            '+' | '-' | '*' | '/' | '^' | '(' | ')' | ',' | '?' | ':' => {
                let kind = match c {
                    '+' => TokenKind::Plus,
                    '-' => TokenKind::Minus,
                    '*' => TokenKind::Mul,
                    '/' => TokenKind::Div,
                    '^' => TokenKind::Pow,
                    '(' => TokenKind::LParen,
                    ')' => TokenKind::RParen,
                    ',' => TokenKind::Comma,
                    '?' => TokenKind::Question,
                    _ => TokenKind::Colon,
                };
                i += 1;
                kind
            }
            '0'..='9' | '.' => {
                let start = i;
                let mut has_digit = false;

                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                    has_digit = true;
                }
                if i < chars.len() && chars[i] == '.' {
                    i += 1;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                        has_digit = true;
                    }
                    if !has_digit {
                        return Err(format!("expected digit after '.' at position {}", start));
                    }
                }

                // Scientific notation: 1e-3, 2.5E+4, ...
                if i < chars.len() && (chars[i] == 'e' || chars[i] == 'E') {
                    let exponent_pos = i;
                    i += 1;
                    if i < chars.len() && (chars[i] == '+' || chars[i] == '-') {
                        i += 1;
                    }
                    let digits_start = i;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                    if i == digits_start {
                        return Err(format!(
                            "invalid exponent in number at position {}",
                            exponent_pos
                        ));
                    }
                }

                let text: String = chars[start..i].iter().collect();
                let value = text
                    .parse::<f64>()
                    .map_err(|_| format!("invalid number '{}' at position {}", text, start))?;
                if !value.is_finite() {
                    return Err(format!("number out of range at position {}", start));
                }
                TokenKind::Number(value)
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                TokenKind::Ident(chars[start..i].iter().collect())
            }
            _ => return Err(format!("unexpected character '{}' at position {}", c, i)),
        };

        tokens.push(Token {
            kind,
            pos: token_pos,
        });
    }

    if tokens.len() > MAX_TOKENS {
        return Err(format!(
            "expression is too long ({} tokens, maximum {})",
            tokens.len(),
            MAX_TOKENS
        ));
    }
    Ok(tokens)
}

// ---------------------------------------------------------------------------
// Parser (AST)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum UnaryOp {
    Neg,
}

#[derive(Clone, Copy)]
enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

enum Expr {
    Number(f64),
    X,
    Unary(UnaryOp, Box<Expr>),
    Binary(BinaryOp, Box<Expr>, Box<Expr>, usize),
    Call {
        name: String,
        args: Vec<Expr>,
        pos: usize,
    },
    If {
        cond: Box<Expr>,
        then_: Box<Expr>,
        else_: Box<Expr>,
    },
}

/// Grammar (lowest to highest precedence):
/// `expr := ternary`
/// `ternary := add ('?' expr ':' expr)?`
/// `add  := mul (('+' | '-') mul)*`
/// `mul  := unary (('*' | '/') unary)*`
/// `unary := ('+' | '-') unary | power`
/// `power := primary ('^' unary)?`  // right-associative, allows 2^-3
/// `primary := number | ident | ident '(' args ')' | '(' expr ')'`
struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    end: usize,
    depth: usize,
}

fn function_arity(name: &str) -> Option<usize> {
    match name {
        "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "sinh" | "cosh" | "tanh" | "exp"
        | "ln" | "log" | "log10" | "sqrt" | "abs" | "floor" | "ceil" | "round" | "sign"
        | "fract" | "square" | "saw" | "tri" | "step" | "sec" | "csc" | "cot" | "deg" | "rad" => {
            Some(1)
        }
        "min" | "max" | "pow" | "mod" | "atan2" | "hypot" | "clamp" => Some(2),
        "lerp" => Some(3),
        _ => None,
    }
}

fn parse_expression(input: &str) -> Result<Expr, String> {
    let tokens = tokenize(input)?;
    let end = input.chars().count();
    if tokens.is_empty() {
        return Err("empty expression at position 0".to_owned());
    }

    let mut parser = Parser {
        tokens,
        pos: 0,
        end,
        depth: 0,
    };
    let expr = parser.parse_expr()?;
    if let Some(tok) = parser.peek() {
        return Err(format!(
            "unexpected token {} at position {}",
            tok.describe(),
            tok.pos
        ));
    }
    Ok(expr)
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn take(&mut self) -> Option<Token> {
        let tok = self.tokens.get(self.pos).cloned();
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    fn consume(&mut self, kind: &TokenKind) -> bool {
        if self.peek().map(|t| &t.kind) == Some(kind) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn enter(&mut self) -> Result<(), String> {
        if self.depth >= MAX_DEPTH {
            let pos = self.peek().map_or(self.end, |t| t.pos);
            return Err(format!(
                "expression is nested too deeply at position {}",
                pos
            ));
        }
        self.depth += 1;
        Ok(())
    }

    fn unexpected_end(&self) -> String {
        format!("unexpected end of expression at position {}", self.end)
    }

    fn expected(&self, what: &str) -> String {
        match self.peek() {
            Some(tok) => format!(
                "expected {} but found {} at position {}",
                what,
                tok.describe(),
                tok.pos
            ),
            None => format!(
                "expected {} but found end of expression at position {}",
                what, self.end
            ),
        }
    }

    fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_ternary()
    }

    fn parse_ternary(&mut self) -> Result<Expr, String> {
        let cond = self.parse_add()?;
        if self.peek().map(|t| &t.kind) == Some(&TokenKind::Question) {
            self.pos += 1;
            let then_ = self.parse_expr()?;
            if !self.consume(&TokenKind::Colon) {
                return Err(self.expected("':'"));
            }
            let else_ = self.parse_expr()?;
            Ok(Expr::If {
                cond: Box::new(cond),
                then_: Box::new(then_),
                else_: Box::new(else_),
            })
        } else {
            Ok(cond)
        }
    }

    fn parse_add(&mut self) -> Result<Expr, String> {
        let mut lhs = self.parse_mul()?;
        loop {
            let op = match self.peek().map(|t| &t.kind) {
                Some(TokenKind::Plus) => BinaryOp::Add,
                Some(TokenKind::Minus) => BinaryOp::Sub,
                _ => break,
            };
            let pos = self.peek().map_or(self.end, |t| t.pos);
            self.pos += 1;
            let rhs = self.parse_mul()?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs), pos);
        }
        Ok(lhs)
    }

    fn parse_mul(&mut self) -> Result<Expr, String> {
        let mut lhs = self.parse_unary()?;
        loop {
            let op = match self.peek().map(|t| &t.kind) {
                Some(TokenKind::Mul) => BinaryOp::Mul,
                Some(TokenKind::Div) => BinaryOp::Div,
                _ => break,
            };
            let pos = self.peek().map_or(self.end, |t| t.pos);
            self.pos += 1;
            let rhs = self.parse_unary()?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs), pos);
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        self.enter()?;
        let result = self.parse_unary_inner();
        self.depth -= 1;
        result
    }

    fn parse_unary_inner(&mut self) -> Result<Expr, String> {
        match self.peek().map(|t| &t.kind) {
            Some(TokenKind::Plus) => {
                self.pos += 1;
                self.parse_unary()
            }
            Some(TokenKind::Minus) => {
                self.pos += 1;
                let operand = self.parse_unary()?;
                Ok(Expr::Unary(UnaryOp::Neg, Box::new(operand)))
            }
            _ => self.parse_power(),
        }
    }

    fn parse_power(&mut self) -> Result<Expr, String> {
        let base = self.parse_primary()?;
        if self.peek().map(|t| &t.kind) == Some(&TokenKind::Pow) {
            let pos = self.peek().map_or(self.end, |t| t.pos);
            self.pos += 1;
            // `unary` here makes `^` right-associative and accepts 2^-3.
            let exponent = self.parse_unary()?;
            Ok(Expr::Binary(
                BinaryOp::Pow,
                Box::new(base),
                Box::new(exponent),
                pos,
            ))
        } else {
            Ok(base)
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        self.enter()?;
        let result = self.parse_primary_inner();
        self.depth -= 1;
        result
    }

    fn parse_primary_inner(&mut self) -> Result<Expr, String> {
        let tok = self.take().ok_or_else(|| self.unexpected_end())?;
        match tok.kind {
            TokenKind::Number(n) => Ok(Expr::Number(n)),
            TokenKind::Ident(name) => {
                if self.consume(&TokenKind::LParen) {
                    let mut args = Vec::new();
                    if !self.consume(&TokenKind::RParen) {
                        loop {
                            args.push(self.parse_expr()?);
                            if !self.consume(&TokenKind::Comma) {
                                break;
                            }
                        }
                        if !self.consume(&TokenKind::RParen) {
                            return Err(self.expected("')'"));
                        }
                    }
                    match function_arity(&name) {
                        None => {
                            return Err(format!(
                                "unknown function '{}' at position {}",
                                name, tok.pos
                            ));
                        }
                        Some(expected) if expected != args.len() => {
                            return Err(format!(
                                "function '{}' expects {} argument{}, got {} at position {}",
                                name,
                                expected,
                                if expected == 1 { "" } else { "s" },
                                args.len(),
                                tok.pos
                            ));
                        }
                        _ => {}
                    }
                    Ok(Expr::Call {
                        name,
                        args,
                        pos: tok.pos,
                    })
                } else {
                    match name.as_str() {
                        "x" => Ok(Expr::X),
                        "pi" => Ok(Expr::Number(PI)),
                        "e" => Ok(Expr::Number(E)),
                        "tau" => Ok(Expr::Number(TAU)),
                        _ => Err(format!(
                            "unknown identifier '{}' at position {}",
                            name, tok.pos
                        )),
                    }
                }
            }
            TokenKind::LParen => {
                let inner = self.parse_expr()?;
                if !self.consume(&TokenKind::RParen) {
                    return Err(self.expected("')'"));
                }
                Ok(inner)
            }
            _ => Err(format!(
                "unexpected token {} at position {}",
                tok.describe(),
                tok.pos
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

impl Expr {
    #[allow(dead_code)]
    fn eval(&self, x: f64) -> Result<f64, String> {
        self.eval_with(x, true)
    }

    /// `strict_division`: when true, division by zero is an error; when false,
    /// IEEE infinity/NaN is returned (used by `sample` to keep singular points).
    fn eval_with(&self, x: f64, strict_division: bool) -> Result<f64, String> {
        match self {
            Expr::Number(v) => Ok(*v),
            Expr::X => Ok(x),
            Expr::Unary(UnaryOp::Neg, inner) => Ok(-inner.eval_with(x, strict_division)?),
            Expr::Binary(op, lhs, rhs, pos) => {
                let a = lhs.eval_with(x, strict_division)?;
                let b = rhs.eval_with(x, strict_division)?;
                match op {
                    BinaryOp::Add => Ok(a + b),
                    BinaryOp::Sub => Ok(a - b),
                    BinaryOp::Mul => Ok(a * b),
                    BinaryOp::Div => {
                        if b == 0.0 && strict_division {
                            Err(format!("division by zero at position {}", pos))
                        } else {
                            Ok(a / b)
                        }
                    }
                    BinaryOp::Pow => Ok(a.powf(b)),
                }
            }
            Expr::Call { name, args, pos } => {
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    values.push(arg.eval_with(x, strict_division)?);
                }
                match function_arity(name) {
                    Some(_) => Ok(apply_function(name, &values)),
                    None => Err(format!("unknown function '{}' at position {}", name, pos)),
                }
            }
            Expr::If { cond, then_, else_ } => {
                let c = cond.eval_with(x, strict_division)?;
                if c != 0.0 && !c.is_nan() {
                    then_.eval_with(x, strict_division)
                } else {
                    else_.eval_with(x, strict_division)
                }
            }
        }
    }
}

fn apply_function(name: &str, args: &[f64]) -> f64 {
    let a = args.first().copied().unwrap_or(f64::NAN);
    let b = args.get(1).copied().unwrap_or(f64::NAN);
    let c = args.get(2).copied().unwrap_or(f64::NAN);
    match name {
        "sin" => a.sin(),
        "cos" => a.cos(),
        "tan" => a.tan(),
        "asin" => a.asin(),
        "acos" => a.acos(),
        "atan" => a.atan(),
        "sinh" => a.sinh(),
        "cosh" => a.cosh(),
        "tanh" => a.tanh(),
        "exp" => a.exp(),
        // `log` is the natural logarithm; `log10` is base 10.
        "ln" | "log" => a.ln(),
        "log10" => a.log10(),
        "sqrt" => a.sqrt(),
        "abs" => a.abs(),
        "floor" => a.floor(),
        "ceil" => a.ceil(),
        "round" => a.round(),
        "fract" => a - a.floor(),
        "sec" => 1.0 / a.cos(),
        "csc" => 1.0 / a.sin(),
        "cot" => 1.0 / a.tan(),
        "deg" => a.to_degrees(),
        "rad" => a.to_radians(),
        "square" => {
            if a.sin() >= 0.0 {
                1.0
            } else {
                -1.0
            }
        }
        "saw" => 2.0 * (a / TAU - (a / TAU + 0.5).floor()),
        "tri" => 2.0 / PI * (a.sin()).asin(),
        "step" => {
            if a >= 0.0 {
                1.0
            } else {
                0.0
            }
        }
        "sign" => {
            if a.is_nan() {
                f64::NAN
            } else if a > 0.0 {
                1.0
            } else if a < 0.0 {
                -1.0
            } else {
                0.0
            }
        }
        "min" => a.min(b),
        "max" => a.max(b),
        "pow" => a.powf(b),
        "mod" => a.rem_euclid(b),
        "atan2" => a.atan2(b),
        "hypot" => a.hypot(b),
        "clamp" => a.clamp(b.min(c), b.max(c)),
        "lerp" => a + (b - a) * c,
        _ => f64::NAN, // unreachable: arity checked in the parser
    }
}

/// Evaluate `expr` at a single value of `x`.
///
/// Returns a descriptive error (with a 0-based character position) for syntax
/// problems, unknown names, wrong argument counts and division by zero.
/// Math domain errors yield `NaN` or infinity instead of an error.
#[allow(dead_code)]
pub fn evaluate(expr: &str, x: f64) -> Result<f64, String> {
    parse_expression(expr)?.eval(x)
}

/// Sample `expr` at `count` evenly spaced points from `x_min` to `x_max`
/// (both endpoints included).
///
/// The expression is parsed once. Points where division by zero occurs are
/// kept as infinities/NaN, so the returned vector always has exactly `count`
/// entries; callers can filter non-finite values when drawing.
pub fn sample(expr: &str, x_min: f64, x_max: f64, count: usize) -> Result<Vec<(f64, f64)>, String> {
    if count < 2 {
        return Err(format!("count must be at least 2, got {}", count));
    }
    if !x_min.is_finite() || !x_max.is_finite() {
        return Err("range bounds must be finite".to_owned());
    }
    if x_min >= x_max {
        return Err(format!(
            "invalid range: x_min ({}) must be less than x_max ({})",
            x_min, x_max
        ));
    }

    let parsed = parse_expression(expr)?;
    let span = x_max - x_min;
    let mut points = Vec::with_capacity(count);
    for i in 0..count {
        let x = if i + 1 == count {
            x_max
        } else {
            x_min + span * (i as f64) / ((count - 1) as f64)
        };
        points.push((x, parsed.eval_with(x, false)?));
    }
    Ok(points)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-12 * (1.0 + a.abs().max(b.abs()))
    }

    fn approx(actual: Result<f64, String>, expected: f64) {
        match actual {
            Ok(v) => assert!(close(v, expected), "expected {}, got {}", expected, v),
            Err(e) => panic!("unexpected error: {}", e),
        }
    }

    #[test]
    fn precedence_and_associativity() {
        approx(evaluate("2 + 3 * 4", 0.0), 14.0);
        approx(evaluate("2 * 3 + 4", 0.0), 10.0);
        approx(evaluate("(2 + 3) * 4", 0.0), 20.0);
        approx(evaluate("10 - 3 - 2", 0.0), 5.0); // left-assoc
        approx(evaluate("100 / 5 / 2", 0.0), 10.0); // left-assoc
        approx(evaluate("2^3^2", 0.0), 512.0); // right-assoc
        approx(evaluate("(2^3)^2", 0.0), 64.0);
        approx(evaluate("8 / 2 * 3", 0.0), 12.0);
    }

    #[test]
    fn unary_minus_and_signs() {
        approx(evaluate("-2^2", 0.0), -4.0);
        approx(evaluate("(-2)^2", 0.0), 4.0);
        approx(evaluate("2^-2", 0.0), 0.25);
        approx(evaluate("--3", 0.0), 3.0);
        approx(evaluate("+-+3", 0.0), -3.0);
        approx(evaluate("-x", 4.0), -4.0);
        approx(evaluate("3 - -2", 0.0), 5.0);
    }

    #[test]
    fn variables_and_constants() {
        approx(evaluate("x + 1", 2.0), 3.0);
        approx(evaluate("x * x", 5.0), 25.0);
        approx(evaluate("pi", 0.0), PI);
        approx(evaluate("e", 0.0), E);
        approx(evaluate("tau", 0.0), TAU);
        approx(evaluate("2 * pi", 0.0), 2.0 * PI);
        approx(evaluate("tau - 2*pi", 0.0), 0.0);
    }

    #[test]
    fn functions() {
        approx(evaluate("sin(0)", 0.0), 0.0);
        approx(evaluate("cos(0)", 0.0), 1.0);
        approx(evaluate("tan(pi/4)", 0.0), 1.0);
        approx(evaluate("asin(1)", 0.0), PI / 2.0);
        approx(evaluate("acos(1)", 0.0), 0.0);
        approx(evaluate("atan(1)", 0.0), PI / 4.0);
        approx(evaluate("sinh(0)", 0.0), 0.0);
        approx(evaluate("cosh(0)", 0.0), 1.0);
        approx(evaluate("tanh(0)", 0.0), 0.0);
        approx(evaluate("exp(1)", 0.0), E);
        approx(evaluate("sqrt(9)", 0.0), 3.0);
        approx(evaluate("abs(-4)", 0.0), 4.0);
        approx(evaluate("floor(-1.2)", 0.0), -2.0);
        approx(evaluate("ceil(1.2)", 0.0), 2.0);
        approx(evaluate("round(1.6)", 0.0), 2.0);
        approx(evaluate("sign(-7)", 0.0), -1.0);
        approx(evaluate("sign(0)", 0.0), 0.0);
        approx(evaluate("sign(3)", 0.0), 1.0);
    }

    #[test]
    fn logarithms_and_multi_arg_functions() {
        approx(evaluate("ln(1)", 0.0), 0.0);
        approx(evaluate("ln(e)", 0.0), 1.0);
        approx(evaluate("log(e^2)", 0.0), 2.0); // log == natural log
        approx(evaluate("log10(1000)", 0.0), 3.0);
        approx(evaluate("min(3, 5)", 0.0), 3.0);
        approx(evaluate("max(3, 5)", 0.0), 5.0);
        approx(evaluate("pow(2, 10)", 0.0), 1024.0);
        approx(evaluate("min(max(1, 4), 3)", 0.0), 3.0);
        approx(evaluate("pow(x, 3)", -2.0), -8.0);
    }

    #[test]
    fn numbers_and_whitespace() {
        approx(evaluate("1e-3", 0.0), 0.001);
        approx(evaluate("1.5e+2", 0.0), 150.0);
        approx(evaluate("2.5E2", 0.0), 250.0);
        approx(evaluate(".5 + 1.", 0.0), 1.5);
        approx(evaluate("  2  +	3  ", 0.0), 5.0);
    }

    #[test]
    fn non_finite_results_are_values_not_errors() {
        let nan = evaluate("sqrt(-1)", 0.0).unwrap();
        assert!(nan.is_nan());
        assert_eq!(evaluate("ln(0)", 0.0).unwrap(), f64::NEG_INFINITY);
        assert!(evaluate("asin(2)", 0.0).unwrap().is_nan());
        // Single-point division by zero is an error in `evaluate`.
        assert!(
            evaluate("1/0", 0.0)
                .unwrap_err()
                .contains("division by zero")
        );
        assert!(
            evaluate("1/x", 0.0)
                .unwrap_err()
                .contains("division by zero")
        );
        // ... but sampling keeps the singular point.
        let points = sample("1/x", -1.0, 1.0, 3).unwrap();
        assert_eq!(points.len(), 3);
        assert!(points[1].1.is_infinite());
    }

    #[test]
    fn syntax_errors_and_positions() {
        assert!(evaluate("", 0.0).unwrap_err().contains("empty expression"));
        assert!(
            evaluate("2 +", 0.0)
                .unwrap_err()
                .contains("unexpected end of expression")
        );
        assert!(evaluate("(2 + 3", 0.0).unwrap_err().contains("expected"));
        assert!(evaluate("2 + 3)", 0.0).unwrap_err().contains("position 5"));
        assert!(
            evaluate("2 @ 3", 0.0)
                .unwrap_err()
                .contains("unexpected character '@' at position 2")
        );
        assert!(
            evaluate("2 3", 0.0)
                .unwrap_err()
                .contains("unexpected token number '3' at position 2")
        );
        assert!(evaluate("1e", 0.0).unwrap_err().contains("exponent"));
        assert!(evaluate(".e", 0.0).unwrap_err().contains("digit"));
    }

    #[test]
    fn unknown_names_and_bad_arity() {
        assert!(
            evaluate("y", 0.0)
                .unwrap_err()
                .contains("unknown identifier 'y' at position 0")
        );
        assert!(
            evaluate("foo(1)", 0.0)
                .unwrap_err()
                .contains("unknown function 'foo' at position 0")
        );
        assert!(
            evaluate("sin", 0.0)
                .unwrap_err()
                .contains("unknown identifier 'sin'")
        );
        assert!(
            evaluate("sin(1, 2)", 0.0)
                .unwrap_err()
                .contains("expects 1 argument, got 2")
        );
        assert!(
            evaluate("min(1)", 0.0)
                .unwrap_err()
                .contains("expects 2 arguments, got 1")
        );
        assert!(
            evaluate("max(1,2,3)", 0.0)
                .unwrap_err()
                .contains("expects 2 arguments, got 3")
        );
        assert!(evaluate("pow(2)", 0.0).is_err());
    }

    #[test]
    fn nesting_limit() {
        let deep = format!("{}1{}", "(".repeat(300), ")".repeat(300));
        assert!(
            evaluate(&deep, 0.0)
                .unwrap_err()
                .contains("nested too deeply")
        );
        let long_unary = format!("{}1", "-".repeat(300));
        assert!(evaluate(&long_unary, 0.0).is_err());
    }

    #[test]
    fn sample_spacing_and_endpoints() {
        let points = sample("x^2", -1.0, 1.0, 5).unwrap();
        assert_eq!(points.len(), 5);
        approx(Ok(points[0].0), -1.0);
        approx(Ok(points[4].0), 1.0);
        for (i, (x, y)) in points.iter().enumerate() {
            approx(Ok(*x), -1.0 + i as f64 * 0.5);
            approx(Ok(*y), x * x);
        }
        let two = sample("sin(x)", 0.0, PI / 2.0, 2).unwrap();
        assert_eq!(two.len(), 2);
        approx(Ok(two[1].1), 1.0);
    }

    #[test]
    fn sample_validation() {
        assert!(
            sample("x", 1.0, 1.0, 10)
                .unwrap_err()
                .contains("invalid range")
        );
        assert!(
            sample("x", 2.0, 1.0, 10)
                .unwrap_err()
                .contains("invalid range")
        );
        assert!(sample("x", 0.0, 1.0, 0).is_err());
        assert!(
            sample("x", 0.0, 1.0, 1)
                .unwrap_err()
                .contains("count must be at least 2")
        );
        assert!(sample("x", f64::NAN, 1.0, 10).is_err());
        assert!(sample("bogus", 0.0, 1.0, 10).is_err());
    }

    #[test]
    fn presets_are_valid() {
        assert!(PRESETS.len() >= 10);
        for (name, expr, xmin, xmax, _, _) in PRESETS {
            assert!(!name.is_empty());
            let points = sample(expr, *xmin, *xmax, 64).unwrap();
            assert_eq!(points.len(), 64, "preset '{}' failed", name);
        }
    }
}
