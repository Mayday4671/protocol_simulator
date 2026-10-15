//! Small, bounded integer expression language. No script engine or external calls.
use super::ProtocolError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Expr {
    Number(i128),
    Ref(String),
    Len(String),
    Bit(Box<Expr>, Box<Expr>),
    Unary(String, Box<Expr>),
    Binary(String, Box<Expr>, Box<Expr>),
}
fn error(message: impl Into<String>) -> ProtocolError {
    ProtocolError::new("expression", message)
}

impl Expr {
    pub fn parse(source: &str) -> Result<Self, ProtocolError> {
        if source.len() > 512 {
            return Err(error("表达式超过512字节"));
        }
        let mut tokens = Vec::<String>::new();
        let bytes = source.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i].is_ascii_whitespace() {
                i += 1;
                continue;
            }
            let start = i;
            if bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' {
                i += 1;
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric() || b"_.".contains(&bytes[i]))
                {
                    i += 1;
                }
            } else {
                if !b"+-*/%()!,<>=&|".contains(&bytes[i]) {
                    return Err(error("不允许的表达式字符"));
                }
                i += 1;
                if i < bytes.len()
                    && matches!(&source[start..=i], "==" | "!=" | "<=" | ">=" | "&&" | "||")
                {
                    i += 1;
                }
            }
            tokens.push(source[start..i].into());
            if tokens.len() > 128 {
                return Err(error("表达式超过128个词元"));
            }
        }
        let mut parser = Parser { tokens, pos: 0 };
        let result = parser.expression(0, 0)?;
        if parser.pos != parser.tokens.len() {
            return Err(error("表达式有多余内容"));
        }
        Ok(result)
    }
    pub fn refs(&self, out: &mut Vec<String>) {
        match self {
            Self::Ref(r) => out.push(r.clone()),
            Self::Len(r) => out.push(format!("len:{r}")),
            Self::Bit(a, b) | Self::Binary(_, a, b) => {
                a.refs(out);
                b.refs(out);
            }
            Self::Unary(_, a) => a.refs(out),
            _ => {}
        }
    }
    pub fn evaluate(
        &self,
        get: &impl Fn(&str) -> Option<i128>,
        len: &impl Fn(&str) -> Option<usize>,
    ) -> Result<i128, ProtocolError> {
        self.eval(get, len, &mut 256)
    }
    fn eval(
        &self,
        get: &impl Fn(&str) -> Option<i128>,
        len: &impl Fn(&str) -> Option<usize>,
        budget: &mut usize,
    ) -> Result<i128, ProtocolError> {
        if *budget == 0 {
            return Err(error("表达式运算步数超限"));
        }
        *budget -= 1;
        Ok(match self {
            Self::Number(v) => *v,
            Self::Ref(r) => get(r).ok_or_else(|| error(format!("字段引用不可用: {r}")))?,
            Self::Len(r) => len(r).ok_or_else(|| error(format!("长度引用不可用: {r}")))? as i128,
            Self::Bit(a, b) => {
                let a = a.eval(get, len, budget)?;
                let b = b.eval(get, len, budget)?;
                if !(0..64).contains(&b) {
                    return Err(error("bit索引必须在0..63"));
                }
                (a >> b) & 1
            }
            Self::Unary(op, a) => {
                let a = a.eval(get, len, budget)?;
                match op.as_str() {
                    "!" => (a == 0) as i128,
                    "-" => a.checked_neg().ok_or_else(|| error("整数溢出"))?,
                    _ => a,
                }
            }
            Self::Binary(op, a, b) => {
                let a = a.eval(get, len, budget)?;
                if op == "&&" && a == 0 {
                    return Ok(0);
                }
                if op == "||" && a != 0 {
                    return Ok(1);
                }
                let b = b.eval(get, len, budget)?;
                match op.as_str() {
                    "+" => a.checked_add(b),
                    "-" => a.checked_sub(b),
                    "*" => a.checked_mul(b),
                    "/" => a.checked_div(b),
                    "%" => a.checked_rem(b),
                    "==" => Some((a == b) as i128),
                    "!=" => Some((a != b) as i128),
                    "<" => Some((a < b) as i128),
                    ">" => Some((a > b) as i128),
                    "<=" => Some((a <= b) as i128),
                    ">=" => Some((a >= b) as i128),
                    "&&" => Some((b != 0) as i128),
                    "||" => Some((b != 0) as i128),
                    _ => None,
                }
                .ok_or_else(|| error("表达式除零或整数溢出"))?
            }
        })
    }
}
struct Parser {
    tokens: Vec<String>,
    pos: usize,
}
impl Parser {
    fn take(&mut self) -> Result<String, ProtocolError> {
        let x = self
            .tokens
            .get(self.pos)
            .cloned()
            .ok_or_else(|| error("表达式不完整"))?;
        self.pos += 1;
        Ok(x)
    }
    fn expect(&mut self, token: &str) -> Result<(), ProtocolError> {
        if self.take()? != token {
            return Err(error(format!("预期 {token}")));
        }
        Ok(())
    }
    fn expression(&mut self, min: u8, depth: usize) -> Result<Expr, ProtocolError> {
        if depth > 16 {
            return Err(error("表达式嵌套超过16层"));
        }
        let token = self.take()?;
        let mut lhs = match token.as_str() {
            "(" => {
                let value = self.expression(0, depth + 1)?;
                self.expect(")")?;
                value
            }
            "!" | "-" | "+" => Expr::Unary(token, Box::new(self.expression(7, depth + 1)?)),
            "bit" if self.tokens.get(self.pos).is_some_and(|x| x == "(") => {
                self.pos += 1;
                let a = self.expression(0, depth + 1)?;
                self.expect(",")?;
                let b = self.expression(0, depth + 1)?;
                self.expect(")")?;
                Expr::Bit(Box::new(a), Box::new(b))
            }
            "len" if self.tokens.get(self.pos).is_some_and(|x| x == "(") => {
                self.pos += 1;
                let r = self.take()?;
                if !identifier(&r) {
                    return Err(error("len只接受字段引用"));
                }
                self.expect(")")?;
                Expr::Len(r)
            }
            x if x.starts_with("0x") || x.starts_with("0X") => Expr::Number(
                i128::from_str_radix(&x[2..], 16).map_err(|_| error("无效十六进制整数"))?,
            ),
            x if x.as_bytes()[0].is_ascii_digit() => {
                Expr::Number(x.parse().map_err(|_| error("无效整数"))?)
            }
            x if identifier(x) => Expr::Ref(x.into()),
            _ => return Err(error("不允许的表达式语法")),
        };
        while let Some(op) = self.tokens.get(self.pos).cloned() {
            let precedence = match op.as_str() {
                "||" => 1,
                "&&" => 2,
                "==" | "!=" => 3,
                "<" | ">" | "<=" | ">=" => 4,
                "+" | "-" => 5,
                "*" | "/" | "%" => 6,
                _ => break,
            };
            if precedence < min {
                break;
            }
            self.pos += 1;
            let rhs = self.expression(precedence + 1, depth + 1)?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }
}
pub(crate) fn identifier(value: &str) -> bool {
    value.split('.').all(|part| {
        !part.is_empty() && part.as_bytes()[0].is_ascii_alphabetic()
            || !part.is_empty() && part.as_bytes()[0] == b'_'
    }) && value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"_.".contains(&b))
}
