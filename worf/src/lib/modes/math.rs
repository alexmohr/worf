use std::sync::{Arc, Mutex, RwLock};

use crate::{
    Error,
    config::{Config, TextOutputMode},
    gui::{
        self, ArcFactory, ArcProvider, DefaultItemFactory, ExpandMode, ItemProvider, MenuItem,
        ProviderData,
    },
};

#[derive(Clone)]
pub(crate) struct MathProvider<T: Clone> {
    menu_item_data: T,
    pub(crate) elements: Vec<MenuItem<T>>,
}

impl<T: Clone> MathProvider<T> {
    pub(crate) fn new(menu_item_data: T) -> Self {
        Self {
            menu_item_data,
            elements: vec![],
        }
    }
    fn add_elements(&mut self, elements: &mut Vec<MenuItem<T>>) {
        self.elements.append(elements);
    }
}

impl<T: Clone> ItemProvider<T> for MathProvider<T> {
    fn get_elements(&mut self, search: Option<&str>) -> ProviderData<T> {
        if let Some(search_text) = search {
            let result = calc(search_text);

            let item = MenuItem::new(
                result,
                None,
                search.map(String::from),
                vec![],
                None,
                0.0,
                Some(self.menu_item_data.clone()),
            );
            let mut result = vec![item];
            result.append(&mut self.elements.clone());
            ProviderData {
                items: Some(result),
            }
        } else {
            ProviderData { items: None }
        }
    }

    fn get_sub_elements(&mut self, _: &MenuItem<T>) -> ProviderData<T> {
        ProviderData { items: None }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Token {
    Int(i64),
    Float(f64),
    Op(char),
    ShiftLeft,
    ShiftRight,
    Power,
}

#[derive(Debug, PartialEq)]
enum Value {
    Int(i64),
    Float(f64),
}

/// Binding power for unary `+` and `-`. Lower than `**` so `-2**2` is `-(2**2)`.
const UNARY_BINDING_POWER: u8 = 13;

/// Left and right binding power of infix operators (higher binds tighter).
/// Follows C/Python precedence: `|` < `^` < `&` < shifts < `+ -` < `* /` < `**`.
/// `**` is right associative.
fn infix_binding_power(op: Token) -> Option<(u8, u8)> {
    match op {
        Token::Op('|') => Some((1, 2)),
        Token::Op('^') => Some((3, 4)),
        Token::Op('&') => Some((5, 6)),
        Token::ShiftLeft | Token::ShiftRight => Some((7, 8)),
        Token::Op('+' | '-') => Some((9, 10)),
        Token::Op('*' | '/') => Some((11, 12)),
        Token::Power => Some((16, 15)),
        _ => None,
    }
}

fn implicit_multiplication_needed(last_token: Option<Token>) -> bool {
    matches!(
        last_token,
        Some(Token::Int(_) | Token::Float(_) | Token::Op(')'))
    )
}

/// Parses an integer literal with the given radix.
/// Values up to `u64::MAX` are accepted and reinterpreted as two's complement,
/// so `0xFFFFFFFFFFFFFFFF` is `-1`.
fn parse_int_literal(digits: &str, radix: u32) -> Result<i64, String> {
    #[allow(clippy::cast_possible_wrap)]
    u64::from_str_radix(digits, radix)
        .map(|n| n as i64)
        .map_err(|_| "Integer literal out of range".to_owned())
}

/// Tokenize an expression string into tokens
fn tokenize(expr: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();

        if c.is_whitespace() {
            i += 1;
            continue;
        }

        let token = match (c, next) {
            ('<', Some('<')) => {
                i += 2;
                Token::ShiftLeft
            }
            ('>', Some('>')) => {
                i += 2;
                Token::ShiftRight
            }
            ('*', Some('*')) => {
                i += 2;
                Token::Power
            }
            ('+' | '-' | '*' | '/' | '&' | '|' | '^' | ')', _) => {
                i += 1;
                Token::Op(c)
            }
            ('(', _) => {
                if implicit_multiplication_needed(tokens.last().copied()) {
                    tokens.push(Token::Op('*'));
                }
                i += 1;
                Token::Op('(')
            }
            ('0', Some(prefix @ ('x' | 'X' | 'b' | 'B'))) => {
                let radix = if prefix.eq_ignore_ascii_case(&'x') {
                    16
                } else {
                    2
                };
                let start = i + 2;
                let mut end = start;
                while end < chars.len() && chars[end].is_digit(radix) {
                    end += 1;
                }
                if start == end {
                    return Err("Invalid integer literal".to_owned());
                }
                let digits: String = chars[start..end].iter().collect();
                if tokens.last() == Some(&Token::Op(')')) {
                    tokens.push(Token::Op('*'));
                }
                i = end;
                Token::Int(parse_int_literal(&digits, radix)?)
            }
            ('0'..='9' | '.', _) => {
                if c == '.' && !next.is_some_and(|n| n.is_ascii_digit()) {
                    return Err("Invalid float literal".to_owned());
                }
                if tokens.last() == Some(&Token::Op(')')) {
                    tokens.push(Token::Op('*'));
                }
                let start = i;
                let mut has_dot = false;
                while i < chars.len()
                    && (chars[i].is_ascii_digit() || (!has_dot && chars[i] == '.'))
                {
                    has_dot |= chars[i] == '.';
                    i += 1;
                }
                let num_str: String = chars[start..i].iter().collect();
                if has_dot {
                    Token::Float(
                        num_str
                            .parse::<f64>()
                            .map_err(|_| "Invalid float literal".to_owned())?,
                    )
                } else {
                    Token::Int(parse_int_literal(&num_str, 10)?)
                }
            }
            _ => return Err("Invalid character in expression".to_owned()),
        };
        tokens.push(token);
    }

    Ok(tokens)
}

fn to_f64(v: &Value) -> f64 {
    match v {
        #[allow(clippy::cast_precision_loss)]
        Value::Int(i) => *i as f64,
        Value::Float(f) => *f,
    }
}

fn to_i64(v: &Value) -> i64 {
    match v {
        Value::Int(i) => *i,
        #[allow(clippy::cast_possible_truncation)]
        Value::Float(f) => *f as i64,
    }
}

fn shift_amount(v: &Value) -> Result<u32, String> {
    u32::try_from(to_i64(v))
        .ok()
        .filter(|s| *s < i64::BITS)
        .ok_or_else(|| format!("Shift amount must be between 0 and {}", i64::BITS - 1))
}

/// Apply an operator to two values
fn apply_op(a: &Value, b: &Value, op: Token) -> Result<Value, String> {
    Ok(match op {
        Token::Op('+') => Value::Float(to_f64(a) + to_f64(b)),
        Token::Op('-') => Value::Float(to_f64(a) - to_f64(b)),
        Token::Op('*') => Value::Float(to_f64(a) * to_f64(b)),
        Token::Op('/') => Value::Float(to_f64(a) / to_f64(b)),
        Token::Power => Value::Float(to_f64(a).powf(to_f64(b))),
        Token::Op('&') => Value::Int(to_i64(a) & to_i64(b)),
        Token::Op('|') => Value::Int(to_i64(a) | to_i64(b)),
        Token::Op('^') => Value::Int(to_i64(a) ^ to_i64(b)),
        Token::ShiftLeft => Value::Int(to_i64(a) << shift_amount(b)?),
        Token::ShiftRight => Value::Int(to_i64(a) >> shift_amount(b)?),
        _ => return Err(format!("Unknown operator {op:?}")),
    })
}

/// Precedence climbing parser that evaluates while parsing
struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.pos).copied();
        self.pos += 1;
        token
    }

    fn parse_operand(&mut self) -> Result<Value, String> {
        match self.next() {
            Some(Token::Int(n)) => Ok(Value::Int(n)),
            Some(Token::Float(f)) => Ok(Value::Float(f)),
            Some(Token::Op('(')) => {
                let value = self.parse(0)?;
                if self.next() == Some(Token::Op(')')) {
                    Ok(value)
                } else {
                    Err("Mismatched parentheses".to_owned())
                }
            }
            Some(Token::Op('-')) => match self.parse(UNARY_BINDING_POWER)? {
                Value::Int(i) => Ok(Value::Int(i.wrapping_neg())),
                Value::Float(f) => Ok(Value::Float(-f)),
            },
            Some(Token::Op('+')) => self.parse(UNARY_BINDING_POWER),
            Some(Token::Op(')')) => Err("Mismatched parentheses".to_owned()),
            Some(_) | None => Err("Missing operand".to_owned()),
        }
    }

    fn parse(&mut self, min_binding_power: u8) -> Result<Value, String> {
        let mut lhs = self.parse_operand()?;
        while let Some(op) = self.tokens.get(self.pos).copied() {
            let Some((left_bp, right_bp)) = infix_binding_power(op) else {
                break;
            };
            if left_bp < min_binding_power {
                break;
            }
            self.pos += 1;
            let rhs = self.parse(right_bp)?;
            lhs = apply_op(&lhs, &rhs, op)?;
        }
        Ok(lhs)
    }
}

/// Evaluate the tokenized expression
fn eval_expr(tokens: Vec<Token>) -> Result<Value, String> {
    let mut parser = Parser { tokens, pos: 0 };
    let value = parser.parse(0)?;
    match parser.next() {
        None => Ok(value),
        Some(Token::Op(')')) => Err("Mismatched parentheses".to_owned()),
        Some(_) => Err("Unexpected token in expression".to_owned()),
    }
}

/// Entry point: takes raw input and evaluates it
fn calc(input: &str) -> String {
    match tokenize(input).and_then(eval_expr) {
        Ok(Value::Int(i)) => format!("{i} (0x{i:X}) (0b{i:b})"),
        Ok(Value::Float(f)) => format!("{f}"),
        Err(e) => e,
    }
}

/// Shows the math mode
/// # Panics
/// When failing to unwrap the arc lock
///
/// # Errors
/// Forwards the errors from `crate::desktop::copy_to_clipboard`
/// if the text output mode is set to `Clipboard`.
pub fn show(config: &Arc<RwLock<Config>>) -> Result<(), Error> {
    let mut calc: Vec<MenuItem<()>> = vec![];
    let provider = Arc::new(Mutex::new(MathProvider::new(())));
    let factory: ArcFactory<()> = Arc::new(Mutex::new(DefaultItemFactory::new()));
    let arc_provider = Arc::clone(&provider) as ArcProvider<()>;
    loop {
        provider.lock().unwrap().add_elements(&mut calc.clone());
        let selection_result = gui::show(
            config,
            Arc::clone(&arc_provider),
            Some(Arc::clone(&factory)),
            None,
            ExpandMode::Verbatim,
            None,
        );

        if let Ok(mi) = selection_result {
            match config.read().unwrap().text_output_mode() {
                TextOutputMode::Clipboard => {
                    crate::desktop::copy_to_clipboard(mi.menu.label, None)?;
                    break;
                }
                TextOutputMode::StandardOutput => {
                    println!("{}", mi.menu.label);
                    break;
                }
                TextOutputMode::None => calc.push(mi.menu),
            }
        } else {
            log::error!("No item selected");
            break;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(input: &str) -> Result<Value, String> {
        tokenize(input).and_then(eval_expr)
    }

    #[test]
    fn test_precedence() {
        assert_eq!(eval("1 + 2 * 3"), Ok(Value::Float(7.0)));
        assert_eq!(eval("(1 + 2) * 3"), Ok(Value::Float(9.0)));
        assert_eq!(eval("1 << 2 + 1"), Ok(Value::Int(8)));
        assert_eq!(eval("6 & 3 | 8"), Ok(Value::Int(10)));
        assert_eq!(eval("2 ** 3 ** 2"), Ok(Value::Float(512.0)));
    }

    #[test]
    fn test_unary_minus() {
        assert_eq!(eval("-5"), Ok(Value::Int(-5)));
        assert_eq!(eval("2 * -3"), Ok(Value::Float(-6.0)));
        assert_eq!(eval("-2 ** 2"), Ok(Value::Float(-4.0)));
        assert_eq!(eval("-(1 + 2)"), Ok(Value::Float(-3.0)));
    }

    #[test]
    fn test_implicit_multiplication() {
        assert_eq!(eval("2(3)"), Ok(Value::Float(6.0)));
        assert_eq!(eval("(2)3"), Ok(Value::Float(6.0)));
        assert_eq!(eval("(2)(3)"), Ok(Value::Float(6.0)));
    }

    #[test]
    fn test_base_literals() {
        assert_eq!(eval("0xff"), Ok(Value::Int(255)));
        assert_eq!(eval("0b101 | 0x10"), Ok(Value::Int(21)));
        assert_eq!(eval("0xFFFFFFFFFFFFFFFF"), Ok(Value::Int(-1)));
        assert!(eval("0x1FFFFFFFFFFFFFFFF").is_err());
        assert!(eval("10x5").is_err());
    }

    #[test]
    fn test_invalid_input_does_not_panic() {
        assert!(eval("3×4").is_err());
        assert!(eval("1÷2").is_err());
        assert!(eval("5€").is_err());
        assert!(eval("99999999999999999999").is_err());
        assert!(eval("1 << 64").is_err());
        assert!(eval("1 << -1").is_err());
        assert!(eval("1 2").is_err());
        assert!(eval("1 +").is_err());
        assert!(eval("(1 + 2").is_err());
        assert!(eval("1 + 2)").is_err());
        assert!(eval("").is_err());
    }

    #[test]
    fn test_calc_output() {
        assert_eq!(calc("1 + 1"), "2");
        assert_eq!(calc("0x10"), "16 (0x10) (0b10000)");
    }
}
