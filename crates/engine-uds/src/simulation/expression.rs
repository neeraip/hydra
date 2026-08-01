//! The expression language (§9.3): one grammar shared by treatment
//! relations, custom groundwater relations, and named expressions —
//! three-level precedence with a right-associative exponent, unary minus
//! negating the multiplicative term it opens, case-insensitive names
//! resolved through the consumer's vocabulary, nineteen functions, and
//! **total** evaluation: domain-guarded operations yield zero, flagged so
//! the consumer can warn once.

use std::fmt;

/// A compiled expression: a postfix program over the consumer's
/// variable slots.
#[derive(Debug, Clone, PartialEq)]
pub struct Expression {
    ops: Vec<Op>,
}

/// One postfix operation.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    Push(f64),
    /// Load the consumer's variable slot.
    Var(usize),
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Neg,
    Fun(Fun),
}

/// The nineteen functions (§9.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fun {
    Sin,
    Cos,
    Tan,
    Cot,
    Asin,
    Acos,
    Atan,
    Acot,
    Sinh,
    Cosh,
    Tanh,
    Coth,
    Abs,
    Sgn,
    Sqrt,
    Log,
    Log10,
    Exp,
    Step,
}

impl Fun {
    fn parse(name: &str) -> Option<Fun> {
        Some(match name {
            "sin" => Fun::Sin,
            "cos" => Fun::Cos,
            "tan" => Fun::Tan,
            "cot" => Fun::Cot,
            "asin" => Fun::Asin,
            "acos" => Fun::Acos,
            "atan" => Fun::Atan,
            "acot" => Fun::Acot,
            "sinh" => Fun::Sinh,
            "cosh" => Fun::Cosh,
            "tanh" => Fun::Tanh,
            "coth" => Fun::Coth,
            "abs" => Fun::Abs,
            "sgn" => Fun::Sgn,
            "sqrt" => Fun::Sqrt,
            "log" => Fun::Log,
            "log10" => Fun::Log10,
            "exp" => Fun::Exp,
            "step" => Fun::Step,
            _ => return None,
        })
    }
}

/// Why an expression fails to compile.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprError {
    /// A name outside the consumer's vocabulary.
    UnknownName(String),
    /// A grammar violation, with a human-readable position note.
    Parse(String),
}

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExprError::UnknownName(n) => write!(f, "unknown name '{n}'"),
            ExprError::Parse(m) => write!(f, "{m}"),
        }
    }
}

/// Tokens of the grammar.
#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Name(String),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Open,
    Close,
}

fn tokenize(text: &str) -> Result<Vec<Token>, ExprError> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        match c {
            ' ' | '\t' => i += 1,
            '+' => {
                out.push(Token::Plus);
                i += 1;
            }
            '-' => {
                out.push(Token::Minus);
                i += 1;
            }
            '*' => {
                out.push(Token::Star);
                i += 1;
            }
            '/' => {
                out.push(Token::Slash);
                i += 1;
            }
            '^' => {
                out.push(Token::Caret);
                i += 1;
            }
            '(' => {
                out.push(Token::Open);
                i += 1;
            }
            ')' => {
                out.push(Token::Close);
                i += 1;
            }
            '0'..='9' | '.' => {
                let start = i;
                while i < bytes.len() && matches!(bytes[i] as char, '0'..='9' | '.') {
                    i += 1;
                }
                // Scientific notation: e/E with an optional sign.
                if i < bytes.len() && matches!(bytes[i] as char, 'e' | 'E') {
                    let mut j = i + 1;
                    if j < bytes.len() && matches!(bytes[j] as char, '+' | '-') {
                        j += 1;
                    }
                    if j < bytes.len() && (bytes[j] as char).is_ascii_digit() {
                        i = j;
                        while i < bytes.len() && (bytes[i] as char).is_ascii_digit() {
                            i += 1;
                        }
                    }
                }
                let s = &text[start..i];
                let v: f64 = s
                    .parse()
                    .map_err(|_| ExprError::Parse(format!("malformed number '{s}'")))?;
                out.push(Token::Number(v));
            }
            'a'..='z' | 'A'..='Z' | '_' => {
                let start = i;
                while i < bytes.len()
                    && matches!(bytes[i] as char, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_')
                {
                    i += 1;
                }
                out.push(Token::Name(text[start..i].to_ascii_lowercase()));
            }
            _ => {
                return Err(ExprError::Parse(format!("unexpected character '{c}'")));
            }
        }
    }
    Ok(out)
}

/// Recursive-descent parser emitting postfix.
struct Parser<'a, R: FnMut(&str) -> Option<usize>> {
    tokens: &'a [Token],
    pos: usize,
    ops: Vec<Op>,
    resolve: R,
}

impl<R: FnMut(&str) -> Option<usize>> Parser<'_, R> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    /// expr := term (('+' | '-') term)*
    fn expr(&mut self) -> Result<(), ExprError> {
        self.term(true)?;
        while let Some(t) = self.peek() {
            match t {
                Token::Plus => {
                    self.pos += 1;
                    self.term(true)?;
                    self.ops.push(Op::Add);
                }
                Token::Minus => {
                    self.pos += 1;
                    self.term(true)?;
                    self.ops.push(Op::Sub);
                }
                _ => break,
            }
        }
        Ok(())
    }

    /// term := ['-' | '+'] factor (('*' | '/') factor)*
    ///
    /// A leading minus — legal only where no operand precedes, which is
    /// exactly the head of a term — negates the whole term (§9.3).
    fn term(&mut self, allow_sign: bool) -> Result<(), ExprError> {
        let mut negate = false;
        if allow_sign {
            match self.peek() {
                Some(Token::Minus) => {
                    negate = true;
                    self.pos += 1;
                }
                Some(Token::Plus) => {
                    self.pos += 1;
                }
                _ => {}
            }
        }
        self.factor()?;
        while let Some(t) = self.peek() {
            match t {
                Token::Star => {
                    self.pos += 1;
                    self.factor()?;
                    self.ops.push(Op::Mul);
                }
                Token::Slash => {
                    self.pos += 1;
                    self.factor()?;
                    self.ops.push(Op::Div);
                }
                _ => break,
            }
        }
        if negate {
            self.ops.push(Op::Neg);
        }
        Ok(())
    }

    /// factor := primary ('^' factor)?   — right-associative.
    fn factor(&mut self) -> Result<(), ExprError> {
        self.primary()?;
        if let Some(Token::Caret) = self.peek() {
            self.pos += 1;
            self.factor()?;
            self.ops.push(Op::Pow);
        }
        Ok(())
    }

    /// primary := number | name | function '(' expr ')' | '(' expr ')'
    fn primary(&mut self) -> Result<(), ExprError> {
        match self.next() {
            Some(Token::Number(v)) => {
                self.ops.push(Op::Push(v));
                Ok(())
            }
            Some(Token::Name(name)) => {
                if let Some(fun) = Fun::parse(&name) {
                    match self.next() {
                        Some(Token::Open) => {}
                        _ => {
                            return Err(ExprError::Parse(format!(
                                "function '{name}' needs a parenthesised argument"
                            )));
                        }
                    }
                    self.expr()?;
                    match self.next() {
                        Some(Token::Close) => {}
                        _ => {
                            return Err(ExprError::Parse(format!(
                                "unbalanced parenthesis after '{name}('"
                            )));
                        }
                    }
                    self.ops.push(Op::Fun(fun));
                    Ok(())
                } else if let Some(slot) = (self.resolve)(&name) {
                    self.ops.push(Op::Var(slot));
                    Ok(())
                } else {
                    Err(ExprError::UnknownName(name))
                }
            }
            Some(Token::Open) => {
                self.expr()?;
                match self.next() {
                    Some(Token::Close) => Ok(()),
                    _ => Err(ExprError::Parse("unbalanced parenthesis".into())),
                }
            }
            other => Err(ExprError::Parse(format!(
                "expected an operand, found {other:?}"
            ))),
        }
    }
}

impl Expression {
    /// Compile `text`, resolving each non-function name through the
    /// consumer's vocabulary to a variable slot. Names are presented
    /// lower-cased (§9.3 case-insensitivity).
    pub fn compile(
        text: &str,
        resolve: impl FnMut(&str) -> Option<usize>,
    ) -> Result<Expression, ExprError> {
        let tokens = tokenize(text)?;
        if tokens.is_empty() {
            return Err(ExprError::Parse("empty expression".into()));
        }
        let mut p = Parser {
            tokens: &tokens,
            pos: 0,
            ops: Vec::new(),
            resolve,
        };
        p.expr()?;
        if p.pos != tokens.len() {
            return Err(ExprError::Parse(format!(
                "trailing input at token {}",
                p.pos + 1
            )));
        }
        Ok(Expression { ops: p.ops })
    }

    /// Evaluate against the consumer's variable slots. Total (§9.3):
    /// domain-guarded operations yield zero; the flag reports whether any
    /// guard fired, for the consumer's once-per-expression warning.
    pub fn eval(&self, vars: &[f64]) -> (f64, bool) {
        let mut stack: Vec<f64> = Vec::with_capacity(8);
        let mut guarded = false;
        // Any non-finite outcome is domain-guarded to zero, subsuming
        // division by zero and NaN-producing arguments (§9.3).
        let total = |x: f64, g: &mut bool| -> f64 {
            if x.is_finite() {
                x
            } else {
                *g = true;
                0.0
            }
        };
        for op in &self.ops {
            match op {
                Op::Push(v) => stack.push(*v),
                Op::Var(i) => stack.push(vars.get(*i).copied().unwrap_or(0.0)),
                Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Pow => {
                    let b = stack.pop().unwrap_or(0.0);
                    let a = stack.pop().unwrap_or(0.0);
                    let r = match op {
                        Op::Add => a + b,
                        Op::Sub => a - b,
                        Op::Mul => a * b,
                        Op::Div => {
                            if b == 0.0 {
                                guarded = true;
                                0.0
                            } else {
                                a / b
                            }
                        }
                        // A non-positive base is guarded to zero (§9.3).
                        Op::Pow => {
                            if a <= 0.0 {
                                if a < 0.0 {
                                    guarded = true;
                                }
                                0.0
                            } else {
                                a.powf(b)
                            }
                        }
                        _ => unreachable!(),
                    };
                    stack.push(total(r, &mut guarded));
                }
                Op::Neg => {
                    let a = stack.pop().unwrap_or(0.0);
                    stack.push(-a);
                }
                Op::Fun(fun) => {
                    let x = stack.pop().unwrap_or(0.0);
                    let r = match fun {
                        Fun::Sin => x.sin(),
                        Fun::Cos => x.cos(),
                        Fun::Tan => x.tan(),
                        Fun::Cot => x.cos() / x.sin(),
                        Fun::Asin => x.asin(),
                        Fun::Acos => x.acos(),
                        Fun::Atan => x.atan(),
                        Fun::Acot => (1.0 / x).atan(),
                        Fun::Sinh => x.sinh(),
                        Fun::Cosh => x.cosh(),
                        Fun::Tanh => x.tanh(),
                        Fun::Coth => x.cosh() / x.sinh(),
                        Fun::Abs => x.abs(),
                        Fun::Sgn => {
                            if x > 0.0 {
                                1.0
                            } else if x < 0.0 {
                                -1.0
                            } else {
                                0.0
                            }
                        }
                        Fun::Sqrt => {
                            if x < 0.0 {
                                guarded = true;
                                0.0
                            } else {
                                x.sqrt()
                            }
                        }
                        Fun::Log => {
                            if x <= 0.0 {
                                guarded = true;
                                0.0
                            } else {
                                x.ln()
                            }
                        }
                        Fun::Log10 => {
                            if x <= 0.0 {
                                guarded = true;
                                0.0
                            } else {
                                x.log10()
                            }
                        }
                        Fun::Exp => x.exp(),
                        Fun::Step => {
                            if x <= 0.0 {
                                0.0
                            } else {
                                1.0
                            }
                        }
                    };
                    stack.push(total(r, &mut guarded));
                }
            }
        }
        let r = stack.pop().unwrap_or(0.0);
        (total(r, &mut guarded), guarded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compile(text: &str) -> Expression {
        // Vocabulary: x → slot 0, y → slot 1.
        Expression::compile(text, |n| match n {
            "x" => Some(0),
            "y" => Some(1),
            _ => None,
        })
        .expect("compile")
    }

    fn eval(text: &str, x: f64, y: f64) -> f64 {
        compile(text).eval(&[x, y]).0
    }

    #[test]
    fn precedence_binds_in_three_levels() {
        assert_eq!(eval("2 + 3 * 4", 0.0, 0.0), 14.0);
        assert_eq!(eval("2 * 3 ^ 2", 0.0, 0.0), 18.0);
        assert_eq!(eval("(2 + 3) * 4", 0.0, 0.0), 20.0);
    }

    #[test]
    fn the_exponent_associates_rightward() {
        // 2^(3^2) = 512, not (2^3)^2 = 64.
        assert_eq!(eval("2 ^ 3 ^ 2", 0.0, 0.0), 512.0);
    }

    #[test]
    fn unary_minus_negates_the_whole_term() {
        assert_eq!(eval("-x * y ^ 2", 3.0, 2.0), -12.0);
        assert_eq!(eval("2 + -3", 0.0, 0.0), -1.0);
        assert_eq!(eval("(-x)", 5.0, 0.0), -5.0);
    }

    #[test]
    fn names_are_case_insensitive_and_scientific_literals_parse() {
        assert_eq!(eval("X + Y", 1.0, 2.0), 3.0);
        assert!((eval("1.5E-2 * x", 100.0, 0.0) - 1.5).abs() < 1e-12);
        assert!((eval("2e3", 0.0, 0.0) - 2000.0).abs() < 1e-12);
    }

    #[test]
    fn functions_evaluate() {
        assert!((eval("sqrt(x)", 9.0, 0.0) - 3.0).abs() < 1e-12);
        assert!((eval("exp(0)", 0.0, 0.0) - 1.0).abs() < 1e-12);
        assert!((eval("log(exp(2))", 0.0, 0.0) - 2.0).abs() < 1e-12);
        assert!((eval("log10(1000)", 0.0, 0.0) - 3.0).abs() < 1e-12);
        assert_eq!(eval("step(x)", 0.5, 0.0), 1.0);
        assert_eq!(eval("step(x)", -0.5, 0.0), 0.0);
        assert_eq!(eval("sgn(x)", -7.0, 0.0), -1.0);
        assert!((eval("cot(x)", 0.5, 0.0) - 0.5_f64.tan().recip()).abs() < 1e-12);
    }

    #[test]
    fn evaluation_is_total_and_flags_the_guard() {
        let e = compile("sqrt(x)");
        let (v, guarded) = e.eval(&[-4.0, 0.0]);
        assert_eq!(v, 0.0);
        assert!(guarded);
        let (v, guarded) = e.eval(&[4.0, 0.0]);
        assert_eq!(v, 2.0);
        assert!(!guarded);
        // Division by zero, log of non-positive, negative-base power.
        assert_eq!(eval("1 / x", 0.0, 0.0), 0.0);
        assert_eq!(eval("log(x)", -1.0, 0.0), 0.0);
        assert_eq!(eval("x ^ 2", -3.0, 0.0), 0.0);
        // A guarded sub-term participates as zero, not as poison.
        assert_eq!(eval("1 + sqrt(-1 * x)", 1.0, 0.0), 1.0);
    }

    #[test]
    fn compile_errors_are_typed() {
        let r = Expression::compile("x + qqq", |n| (n == "x").then_some(0));
        assert_eq!(r, Err(ExprError::UnknownName("qqq".into())));
        assert!(matches!(
            Expression::compile("x +", |_| Some(0)),
            Err(ExprError::Parse(_))
        ));
        assert!(matches!(
            Expression::compile("(x", |_| Some(0)),
            Err(ExprError::Parse(_))
        ));
        assert!(matches!(
            Expression::compile("", |_| Some(0)),
            Err(ExprError::Parse(_))
        ));
    }
}
