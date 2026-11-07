//! Arithmetic expressions over named parameters, used by component
//! geometry: numbers, parameter names, `+ - * / ^`, parentheses, unary
//! minus and the functions `min`, `max`, `abs`, `sqrt`, `round`, `floor`,
//! `ceil`, `sin`, `cos`, `tan` (degrees). Errors are values, never panics;
//! nesting depth and input length are bounded.

const MAX_DEPTH: usize = 64;
/// Longest expression text accepted, in bytes.
pub const MAX_LEN: usize = 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExprError {
    Empty,
    TooLong,
    TooDeep,
    /// Unexpected character or token at the byte offset.
    Syntax(usize),
    UnknownName(String),
    /// Wrong number of arguments for a function.
    Arity(String),
    /// Division by zero, square root of a negative number, overflow…
    NotFinite,
}

impl std::fmt::Display for ExprError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExprError::Empty => write!(f, "empty expression"),
            ExprError::TooLong => write!(f, "expression too long"),
            ExprError::TooDeep => write!(f, "expression nested too deeply"),
            ExprError::Syntax(at) => write!(f, "syntax error at character {}", at + 1),
            ExprError::UnknownName(n) => write!(f, "unknown name '{n}'"),
            ExprError::Arity(n) => write!(f, "wrong number of arguments for '{n}'"),
            ExprError::NotFinite => write!(f, "result is not a finite number"),
        }
    }
}

impl std::error::Error for ExprError {}

/// Evaluate `text`, resolving names with `lookup`.
pub fn eval(text: &str, lookup: &dyn Fn(&str) -> Option<f64>) -> Result<f64, ExprError> {
    if text.len() > MAX_LEN {
        return Err(ExprError::TooLong);
    }
    if text.trim().is_empty() {
        return Err(ExprError::Empty);
    }
    let mut p = Parser { chars: text.char_indices().collect(), i: 0, lookup };
    let v = p.sum(0)?;
    p.skip_ws();
    if p.i < p.chars.len() {
        return Err(ExprError::Syntax(p.offset()));
    }
    finite(v)
}

/// Names referenced by `text` (for dependency checks and renames).
pub fn names(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_digit() || c == '.' {
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            // Skip an exponent so "1e3" does not yield a name "e3".
            if i < chars.len() && matches!(chars[i], 'e' | 'E') {
                let mut j = i + 1;
                if j < chars.len() && matches!(chars[j], '+' | '-') {
                    j += 1;
                }
                if j < chars.len() && chars[j].is_ascii_digit() {
                    i = j;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && is_ident_char(chars[i]) {
                i += 1;
            }
            let name: String = chars[start..i].iter().collect();
            let next = chars[i..].iter().find(|c| !c.is_whitespace());
            if next != Some(&'(') && !out.contains(&name) {
                out.push(name);
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Characters allowed in parameter names (letters of any script, digits, `_`).
pub fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Whether `name` can be used as a parameter name.
pub fn is_valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(is_ident_char)
        && !FUNCTIONS.contains(&name)
}

const FUNCTIONS: [&str; 10] = ["min", "max", "abs", "sqrt", "round", "floor", "ceil", "sin", "cos", "tan"];

fn finite(v: f64) -> Result<f64, ExprError> {
    if v.is_finite() { Ok(v) } else { Err(ExprError::NotFinite) }
}

struct Parser<'a> {
    chars: Vec<(usize, char)>,
    i: usize,
    lookup: &'a dyn Fn(&str) -> Option<f64>,
}

impl Parser<'_> {
    fn offset(&self) -> usize {
        self.chars.get(self.i).map_or_else(|| self.chars.last().map_or(0, |c| c.0 + 1), |c| c.0)
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.i).map(|c| c.1)
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.i += 1;
        }
    }

    fn eat(&mut self, c: char) -> bool {
        self.skip_ws();
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn sum(&mut self, depth: usize) -> Result<f64, ExprError> {
        let mut v = self.product(depth)?;
        loop {
            if self.eat('+') {
                v += self.product(depth)?;
            } else if self.eat('-') {
                v -= self.product(depth)?;
            } else {
                return Ok(v);
            }
        }
    }

    fn product(&mut self, depth: usize) -> Result<f64, ExprError> {
        let mut v = self.power(depth)?;
        loop {
            if self.eat('*') {
                v *= self.power(depth)?;
            } else if self.eat('/') {
                let d = self.power(depth)?;
                if d == 0.0 {
                    return Err(ExprError::NotFinite);
                }
                v /= d;
            } else {
                return Ok(v);
            }
        }
    }

    fn power(&mut self, depth: usize) -> Result<f64, ExprError> {
        let base = self.unary(depth)?;
        if self.eat('^') {
            // Right associative.
            let exp = self.power(depth + 1)?;
            return finite(base.powf(exp));
        }
        Ok(base)
    }

    fn unary(&mut self, depth: usize) -> Result<f64, ExprError> {
        if depth > MAX_DEPTH {
            return Err(ExprError::TooDeep);
        }
        if self.eat('-') {
            return Ok(-self.unary(depth + 1)?);
        }
        if self.eat('+') {
            return self.unary(depth + 1);
        }
        self.atom(depth)
    }

    fn atom(&mut self, depth: usize) -> Result<f64, ExprError> {
        self.skip_ws();
        let start = self.i;
        let Some(c) = self.peek() else { return Err(ExprError::Syntax(self.offset())) };
        if c == '(' {
            self.i += 1;
            let v = self.sum(depth + 1)?;
            if !self.eat(')') {
                return Err(ExprError::Syntax(self.offset()));
            }
            return Ok(v);
        }
        if c.is_ascii_digit() || c == '.' {
            while self.peek().is_some_and(|c| c.is_ascii_digit() || c == '.') {
                self.i += 1;
            }
            // Exponent: 1e-3.
            if matches!(self.peek(), Some('e' | 'E')) {
                let save = self.i;
                self.i += 1;
                if matches!(self.peek(), Some('+' | '-')) {
                    self.i += 1;
                }
                if self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                        self.i += 1;
                    }
                } else {
                    self.i = save;
                }
            }
            let text: String = self.chars[start..self.i].iter().map(|c| c.1).collect();
            return text.parse().map_err(|_| ExprError::Syntax(self.chars[start].0));
        }
        if c.is_alphabetic() || c == '_' {
            while self.peek().is_some_and(is_ident_char) {
                self.i += 1;
            }
            let name: String = self.chars[start..self.i].iter().map(|c| c.1).collect();
            if self.eat('(') {
                let mut args = vec![self.sum(depth + 1)?];
                while self.eat(',') || self.eat(';') {
                    args.push(self.sum(depth + 1)?);
                }
                if !self.eat(')') {
                    return Err(ExprError::Syntax(self.offset()));
                }
                return call(&name, &args);
            }
            return (self.lookup)(&name).ok_or(ExprError::UnknownName(name));
        }
        Err(ExprError::Syntax(self.offset()))
    }
}

fn call(name: &str, args: &[f64]) -> Result<f64, ExprError> {
    let one = || match args {
        [x] => Ok(*x),
        _ => Err(ExprError::Arity(name.to_string())),
    };
    let v = match name {
        "min" | "max" if args.is_empty() => return Err(ExprError::Arity(name.to_string())),
        "min" => args.iter().copied().fold(f64::INFINITY, f64::min),
        "max" => args.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        "abs" => one()?.abs(),
        "sqrt" => {
            let x = one()?;
            if x < 0.0 {
                return Err(ExprError::NotFinite);
            }
            x.sqrt()
        }
        "round" => one()?.round(),
        "floor" => one()?.floor(),
        "ceil" => one()?.ceil(),
        "sin" => one()?.to_radians().sin(),
        "cos" => one()?.to_radians().cos(),
        "tan" => one()?.to_radians().tan(),
        _ => return Err(ExprError::UnknownName(name.to_string())),
    };
    finite(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(s: &str) -> Result<f64, ExprError> {
        eval(s, &|n| match n {
            "Largura" => Some(1.2),
            "altura" => Some(0.75),
            "d_2" => Some(2.0),
            _ => None,
        })
    }

    #[test]
    fn arithmetic_and_precedence() {
        assert_eq!(ev("1 + 2 * 3"), Ok(7.0));
        assert_eq!(ev("(1 + 2) * 3"), Ok(9.0));
        assert_eq!(ev("2 ^ 3 ^ 2"), Ok(512.0));
        assert_eq!(ev("-2 ^ 2"), Ok(4.0));
        assert!((ev("Largura / 2 - 0.05").unwrap() - 0.55).abs() < 1e-12);
        assert_eq!(ev("max(altura, 0.9; 0.1) + min(d_2, 1)"), Ok(1.9));
        assert_eq!(ev("1e-3 * 1000"), Ok(1.0));
        assert_eq!(ev(".5"), Ok(0.5));
        assert!((ev("sin(30)").unwrap() - 0.5).abs() < 1e-12);
        assert_eq!(ev("round(2.6) + floor(1.9) + ceil(0.1) + abs(-1)"), Ok(6.0));
    }

    #[test]
    fn errors_are_reported() {
        assert_eq!(ev(""), Err(ExprError::Empty));
        assert_eq!(ev("1 +"), Err(ExprError::Syntax(3)));
        assert_eq!(ev("2 * (3"), Err(ExprError::Syntax(6)));
        assert_eq!(ev("1 2"), Err(ExprError::Syntax(2)));
        assert_eq!(ev("comprimento"), Err(ExprError::UnknownName("comprimento".into())));
        assert_eq!(ev("1 / 0"), Err(ExprError::NotFinite));
        assert_eq!(ev("sqrt(-1)"), Err(ExprError::NotFinite));
        assert_eq!(ev("abs(1, 2)"), Err(ExprError::Arity("abs".into())));
        assert_eq!(ev("10 ^ 400"), Err(ExprError::NotFinite));
        assert_eq!(ev("1..2").map_err(|_| ()), Err(()));
        assert_eq!(ev(&"(".repeat(200)), Err(ExprError::TooDeep));
        assert_eq!(ev(&"-".repeat(200)), Err(ExprError::TooDeep));
        assert_eq!(ev(&"1+".repeat(600)), Err(ExprError::TooLong));
    }

    #[test]
    fn names_and_validity() {
        assert_eq!(names("Largura / 2 + max(altura, 1e-3) * d_2"), vec!["Largura", "altura", "d_2"]);
        assert!(is_valid_name("Profundidade"));
        assert!(is_valid_name("_x1"));
        assert!(!is_valid_name("1x"));
        assert!(!is_valid_name("a b"));
        assert!(!is_valid_name("max"));
        assert!(!is_valid_name(""));
    }
}
