use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, RwLock},
};

use regex::Regex;

use crate::{
    config::Config,
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
    #[allow(clippy::cast_possible_truncation)]
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

#[derive(Debug, Clone, Copy)]
enum Token {
    Int(i64),
    Float(f64),
    Op(char),
    ShiftLeft,
    ShiftRight,
    Power,
}

#[derive(Debug)]
enum Value {
    Int(i64),
    Float(f64),
}

/// Normalize base literals like 0x and 0b into decimal format
fn normalize_bases(expr: &str) -> String {
    let hex_re = Regex::new(r"0x[0-9a-fA-F]+").unwrap();
    let expr = hex_re.replace_all(expr, |caps: &regex::Captures| {
        i64::from_str_radix(&caps[0][2..], 16).unwrap().to_string()
    });

    let bin_re = Regex::new(r"0b[01]+").unwrap();
    bin_re
        .replace_all(&expr, |caps: &regex::Captures| {
            i64::from_str_radix(&caps[0][2..], 2).unwrap().to_string()
        })
        .to_string()
}

fn insert_implicit_multiplication(tokens: &mut VecDeque<Token>, last_token: Option<&Token>) {
    if matches!(
        last_token,
        Some(Token::Int(_) | Token::Float(_) | Token::Op(')'))
    ) {
        tokens.push_back(Token::Op('*'));
    }
}

/// Tokenize a normalized expression string into tokens
#[allow(clippy::too_many_lines)]
fn tokenize(expr: &str) -> Result<VecDeque<Token>, String> {
    let mut tokens = VecDeque::new();
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;
    let mut last_token: Option<Token> = None;

    while i < chars.len() {
        let c = chars[i];

        if c.is_whitespace() {
            i += 1;
            continue;
        }

        // Multi-character operators
        if i + 1 < chars.len() {
            match &expr[i..=i + 1] {
                "<<" => {
                    tokens.push_back(Token::ShiftLeft);
                    last_token = Some(Token::ShiftLeft);
                    i += 2;
                    continue;
                }
                ">>" => {
                    tokens.push_back(Token::ShiftRight);
                    last_token = Some(Token::ShiftRight);
                    i += 2;
                    continue;
                }
                "**" => {
                    tokens.push_back(Token::Power);
                    last_token = Some(Token::Power);
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }

        // Single-character operators, parentheses, or digits/float
        match c {
            '+' | '-' | '*' | '/' | '&' | '|' | '^' => {
                let token = Token::Op(c);
                tokens.push_back(token);
                last_token = Some(token);
                i += 1;
            }
            '(' => {
                insert_implicit_multiplication(&mut tokens, last_token.as_ref());
                let token = Token::Op('(');
                tokens.push_back(token);
                last_token = Some(token);
                i += 1;
            }
            ')' => {
                let token = Token::Op(')');
                tokens.push_back(token);
                last_token = Some(token);
                i += 1;
            }
            '0'..='9' | '.' => {
                // Only insert implicit multiplication if the last token is ')' and the last token in tokens is not already an operator (except ')')
                if let Some(Token::Op(')')) = last_token {
                    if let Some(Token::Op(op)) = tokens.back() {
                        if *op == ')' {
                            tokens.push_back(Token::Op('*'));
                        }
                    } else {
                        tokens.push_back(Token::Op('*'));
                    }
                }
                let start = i;
                let mut has_dot = c == '.';
                if c == '.' && (i + 1 >= chars.len() || !chars[i + 1].is_ascii_digit()) {
                    return Err("Invalid float literal".to_owned());
                }
                i += 1;
                while i < chars.len()
                    && (chars[i].is_ascii_digit() || (!has_dot && chars[i] == '.'))
                {
                    if chars[i] == '.' {
                        has_dot = true;
                    }
                    i += 1;
                }
                let num_str: String = chars[start..i].iter().collect();
                if has_dot {
                    let n = num_str
                        .parse::<f64>()
                        .map_err(|_| "Invalid float literal".to_owned())?;
                    let token = Token::Float(n);
                    tokens.push_back(token);
                    last_token = Some(token);
                } else {
                    let n = num_str
                        .parse::<i64>()
                        .map_err(|_| "Invalid integer literal".to_owned())?;
                    let token = Token::Int(n);
                    tokens.push_back(token);
                    last_token = Some(token);
                }
            }
            _ => return Err("Invalid character in expression".to_owned()),
        }
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

/// Apply an operator to two values
fn apply_op(a: &Value, b: &Value, op: &Token) -> Value {
    match op {
        Token::Op('+') => Value::Float(to_f64(a) + to_f64(b)),
        Token::Op('-') => Value::Float(to_f64(a) - to_f64(b)),
        Token::Op('*') => Value::Float(to_f64(a) * to_f64(b)),
        Token::Op('/') => Value::Float(to_f64(a) / to_f64(b)),
        Token::Power => Value::Float(to_f64(a).powf(to_f64(b))),
        Token::Op('&') => Value::Int(to_i64(a) & to_i64(b)),
        Token::Op('|') => Value::Int(to_i64(a) | to_i64(b)),
        Token::Op('^') => Value::Int(to_i64(a) ^ to_i64(b)),
        Token::ShiftLeft => Value::Int(to_i64(a) << to_i64(b)),
        Token::ShiftRight => Value::Int(to_i64(a) >> to_i64(b)),
        _ => panic!("Unknown operator"),
    }
}

/// Return precedence of operator (lower number = higher precedence)
fn precedence(op: &Token) -> u8 {
    match op {
        Token::Power => 1,
        Token::ShiftLeft | Token::ShiftRight => 2,
        Token::Op('*' | '/') => 3,
        Token::Op('+' | '-') => 4,
        Token::Op('&') => 5,
        Token::Op('^') => 6,
        Token::Op('|') => 7,
        _ => 100,
    }
}

/// Evaluate the tokenized expression using shunting yard algorithm
fn eval_expr(tokens: &mut VecDeque<Token>) -> Result<Value, String> {
    let mut values = Vec::new();
    let mut ops = Vec::new();

    while let Some(token) = tokens.pop_front() {
        match token {
            Token::Int(n) => values.push(Value::Int(n)),
            Token::Float(f) => values.push(Value::Float(f)),
            Token::Op('(') => {
                ops.push(Token::Op('('));
            }
            Token::Op(')') => {
                while let Some(top_op) = ops.last() {
                    if let Token::Op('(') = top_op {
                        break;
                    }
                    let b = values.pop().ok_or("Missing left operand")?;
                    let a = values.pop().ok_or("Missing right operand")?;
                    let op = ops.pop().ok_or("Missing operator")?;
                    values.push(apply_op(&a, &b, &op));
                }
                if let Some(Token::Op('(')) = ops.last() {
                    ops.pop(); // Remove '('
                } else {
                    return Err("Mismatched parentheses".to_owned());
                }
            }
            op @ (Token::Op(_) | Token::ShiftLeft | Token::ShiftRight | Token::Power) => {
                while let Some(top_op) = ops.last() {
                    // Only pop ops with higher or equal precedence, and not '('
                    if let Token::Op('(') = top_op {
                        break;
                    }
                    if precedence(&op) >= precedence(top_op) {
                        let b = values.pop().ok_or("Missing left operand")?;
                        let a = values.pop().ok_or("Missing right operand")?;
                        let op = ops.pop().ok_or("Missing operator")?;
                        values.push(apply_op(&a, &b, &op));
                    } else {
                        break;
                    }
                }
                ops.push(op);
            }
        }
    }

    // Final reduction: check if there are enough values for the remaining operators
    if !ops.is_empty() && values.len() < 2 {
        return Err(format!(
            "Not enough values for the remaining operators (values: {values:?}, ops: {ops:?})",
        ));
    }
    while let Some(op) = ops.pop() {
        if let Token::Op('(') = op {
            return Err("Mismatched parentheses".to_owned());
        }
        let b = values.pop().ok_or_else(|| {
            format!("Missing right operand in final evaluation (values: {values:?}, ops: {ops:?})",)
        })?;
        let a = values.pop().ok_or_else(|| {
            format!("Missing left operand in final evaluation (values: {values:?}, ops: {ops:?})",)
        })?;
        values.push(apply_op(&a, &b, &op));
    }

    values.pop().ok_or("No result after evaluation".to_owned())
}

/// Entry point: takes raw input, normalizes and evaluates it
fn calc(input: &str) -> String {
    let normalized = normalize_bases(input);
    let mut tokens = match tokenize(&normalized) {
        Ok(t) => t,
        Err(e) => return e,
    };

    match eval_expr(&mut tokens) {
        Ok(Value::Int(i)) => format!("{i} (0x{i:X}) (0b{i:b})"),
        Ok(Value::Float(f)) => format!("{f}"),
        Err(e) => e,
    }
}

/// Shows the math mode
/// # Panics
/// When failing to unwrap the arc lock
pub fn show(config: &Arc<RwLock<Config>>) {
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
            calc.push(mi.menu);
        } else {
            log::error!("No item selected");
            break;
        }
    }
}
