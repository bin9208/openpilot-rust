use super::{insert, Document, Error, Text, Value};

pub(super) struct Parser<'a> {
    pub source: &'a str,
    pub offset: usize,
}
enum Frame {
    Array(Vec<usize>),
    Object(Vec<(Text, usize)>, Text),
}
impl Parser<'_> {
    pub fn error(&self, reason: &'static str) -> Error {
        Error::Syntax {
            offset: self.offset,
            reason,
        }
    }
    pub fn rest(&self) -> &str {
        &self.source[self.offset..]
    }
    pub fn peek(&self) -> Option<u8> {
        self.source.as_bytes().get(self.offset).copied()
    }
    fn whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.offset += 1;
        }
    }
    fn key(&mut self) -> Result<Text, Error> {
        self.whitespace();
        let key = self.text()?;
        self.whitespace();
        if self.peek() != Some(b':') {
            return Err(self.error("expected colon"));
        }
        self.offset += 1;
        Ok(key)
    }
    fn primitive(&mut self) -> Result<Value, Error> {
        match self.peek() {
            Some(b'"') => Ok(Value::Text(self.text()?)),
            Some(b'n') if self.rest().starts_with("null") => {
                self.offset += 4;
                Ok(Value::Null)
            }
            Some(b't') if self.rest().starts_with("true") => {
                self.offset += 4;
                Ok(Value::Bool(true))
            }
            Some(b'f') if self.rest().starts_with("false") => {
                self.offset += 5;
                Ok(Value::Bool(false))
            }
            Some(b'N') if self.rest().starts_with("NaN") => {
                self.offset += 3;
                Ok(Value::Float(f64::NAN))
            }
            Some(b'I') if self.rest().starts_with("Infinity") => {
                self.offset += 8;
                Ok(Value::Float(f64::INFINITY))
            }
            Some(b'-') if self.rest().starts_with("-Infinity") => {
                self.offset += 9;
                Ok(Value::Float(f64::NEG_INFINITY))
            }
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(self.error("expected value")),
        }
    }
    fn number(&mut self) -> Result<Value, Error> {
        let start = self.offset;
        if self.peek() == Some(b'-') {
            self.offset += 1;
        }
        let digits_start = self.offset;
        match self.peek() {
            Some(b'0') => self.offset += 1,
            Some(b'1'..=b'9') => {
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.offset += 1;
                }
            }
            _ => return Err(self.error("expected digit")),
        }
        let digits = self.offset - digits_start;
        let mut floating = false;
        if self.peek() == Some(b'.') {
            floating = true;
            self.offset += 1;
            let fraction = self.offset;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.offset += 1;
            }
            if self.offset == fraction {
                return Err(self.error("expected fraction"));
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            floating = true;
            self.offset += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.offset += 1;
            }
            let exponent = self.offset;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.offset += 1;
            }
            if self.offset == exponent {
                return Err(self.error("expected exponent"));
            }
        }
        let value = &self.source[start..self.offset];
        if floating {
            Ok(Value::Float(
                value.parse().map_err(|_| self.error("invalid float"))?,
            ))
        } else if digits > 4300 {
            Err(Error::IntegerLimit)
        } else {
            Ok(Value::Integer(
                if value == "-0" { "0" } else { value }.to_owned(),
            ))
        }
    }
}

pub(crate) fn parse(source: &str) -> Result<Document, Error> {
    let mut parser = Parser { source, offset: 0 };
    let mut document = Document {
        values: Vec::new(),
        root: 0,
    };
    let mut stack = Vec::new();
    loop {
        parser.whitespace();
        let value = match parser.peek() {
            Some(b'{') => {
                parser.offset += 1;
                parser.whitespace();
                if parser.peek() == Some(b'}') {
                    parser.offset += 1;
                    Value::Object(Vec::new())
                } else {
                    stack.push(Frame::Object(Vec::new(), parser.key()?));
                    continue;
                }
            }
            Some(b'[') => {
                parser.offset += 1;
                parser.whitespace();
                if parser.peek() == Some(b']') {
                    parser.offset += 1;
                    Value::Array(Vec::new())
                } else {
                    stack.push(Frame::Array(Vec::new()));
                    continue;
                }
            }
            _ => parser.primitive()?,
        };
        let mut completed = document.push(value);
        loop {
            parser.whitespace();
            match stack.pop() {
                None => {
                    if parser.peek().is_some() {
                        return Err(parser.error("trailing data"));
                    }
                    document.root = completed;
                    return Ok(document);
                }
                Some(Frame::Array(mut values)) => {
                    values.push(completed);
                    match parser.peek() {
                        Some(b']') => {
                            parser.offset += 1;
                            completed = document.push(Value::Array(values));
                        }
                        Some(b',') => {
                            parser.offset += 1;
                            stack.push(Frame::Array(values));
                            break;
                        }
                        _ => return Err(parser.error("expected array delimiter")),
                    }
                }
                Some(Frame::Object(mut values, key)) => {
                    insert(&mut values, key, completed);
                    match parser.peek() {
                        Some(b'}') => {
                            parser.offset += 1;
                            completed = document.push(Value::Object(values));
                        }
                        Some(b',') => {
                            parser.offset += 1;
                            stack.push(Frame::Object(values, parser.key()?));
                            break;
                        }
                        _ => return Err(parser.error("expected object delimiter")),
                    }
                }
            }
        }
    }
}
