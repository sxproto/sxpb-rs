use std::collections::HashSet;

use crate::value::{ElementKind, element_kind};
use crate::{Field, Lone, MAX_DEPTH, ManyofEntry, Nest, NestItem, Number, ParseError, Value};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Kind<'a> {
    Open,
    Close,
    Dict,
    List,
    Nest,
    Empty,
    Quoted(String),
    Multi(String),
    Atom(&'a str),
    End,
}
#[derive(Clone, Debug)]
struct Token<'a> {
    kind: Kind<'a>,
    offset: usize,
}

fn ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 11 | 12)
}
fn skip_ws(s: &[u8], mut p: usize) -> usize {
    while s.get(p).is_some_and(|&b| ws(b)) {
        p += 1;
    }
    p
}
fn delim(b: u8) -> bool {
    ws(b) || matches!(b, b'(' | b')' | b';' | b'"')
}

// Discriminators admit ASCII blank space but deliberately not comments.
fn marker(source: &str, p: usize) -> Option<(Kind<'_>, usize, usize)> {
    let b = source.as_bytes();
    let i = skip_ws(b, p + 1);
    if b.get(i) == Some(&b'(') {
        let j = skip_ws(b, i + 1);
        if b.get(j) == Some(&b')') {
            let k = skip_ws(b, j + 1);
            if b.get(k) == Some(&b')') {
                return Some((Kind::List, k + 1, 2));
            }
        }
    }
    if b.get(i..i + 2) == Some(b"\"\"") {
        let j = skip_ws(b, i + 2);
        if b.get(j) == Some(&b')') {
            return Some((Kind::Nest, j + 1, 1));
        }
    }
    if b.get(i) == Some(&b')') {
        return Some((Kind::Dict, i + 1, 1));
    }
    None
}

fn quoted(source: &str, start: usize, triple: bool) -> Result<(String, usize), ParseError> {
    let width = if triple { 3 } else { 1 };
    let mut p = start + width;
    let mut result = String::new();
    while let Some(ch) = source[p..].chars().next() {
        if (triple && source[p..].starts_with("\"\"\"")) || (!triple && ch == '"') {
            return Ok((result, p + width));
        }
        p += ch.len_utf8();
        if ch == '\r' {
            continue;
        }
        if ch != '\\' {
            result.push(ch);
            continue;
        }
        // A triple-string delimiter is structural even after a backslash.
        if triple && source[p..].starts_with("\"\"\"") {
            return Err(ParseError::at(source, p, "unterminated escape"));
        }
        let Some(escaped) = source[p..].chars().next() else {
            return Err(ParseError::at(source, p, "unterminated escape"));
        };
        let escape_offset = p - 1;
        p += escaped.len_utf8();
        match escaped {
            '"' => result.push('"'),
            '\\' => result.push('\\'),
            't' => result.push('\t'),
            'n' => result.push('\n'),
            'v' => result.push('\u{b}'),
            'f' => result.push('\u{c}'),
            'r' => result.push('\r'),
            '\n' => {}
            '\r' if source.as_bytes().get(p) == Some(&b'\n') => {
                p += 1;
            }
            _ => {
                return Err(ParseError::at(
                    source,
                    escape_offset,
                    format!("unknown escape \\{escaped}"),
                ));
            }
        }
    }
    Err(ParseError::at(source, start, "unterminated quoted string"))
}

fn tokenize(source: &str) -> Result<Vec<Token<'_>>, ParseError> {
    let b = source.as_bytes();
    let mut tokens = Vec::new();
    let mut p = 0;
    let mut depth: usize = 0;
    while p < b.len() {
        if ws(b[p]) {
            p += 1;
            continue;
        }
        if b[p] == b';' {
            while p < b.len() && b[p] != b'\n' {
                p += 1;
            }
            continue;
        }
        let offset = p;
        let kind = match b[p] {
            b'(' => {
                if let Some((kind, end, internal)) = marker(source, p) {
                    if depth + internal > MAX_DEPTH {
                        return Err(ParseError::at(source, p, "nesting depth limit exceeded"));
                    }
                    p = end;
                    kind
                } else {
                    depth += 1;
                    if depth > MAX_DEPTH {
                        return Err(ParseError::at(source, p, "nesting depth limit exceeded"));
                    }
                    p += 1;
                    Kind::Open
                }
            }
            b')' => {
                depth = depth.saturating_sub(1);
                p += 1;
                Kind::Close
            }
            b'"' => {
                let triple = source[p..].starts_with("\"\"\"");
                if !triple && source[p..].starts_with("\"\"") {
                    p += 2;
                    Kind::Empty
                } else {
                    let (text, end) = quoted(source, p, triple)?;
                    p = end;
                    if triple {
                        Kind::Multi(text)
                    } else {
                        Kind::Quoted(text)
                    }
                }
            }
            _ => {
                if source[p..].starts_with('\u{feff}') {
                    return Err(ParseError::at(
                        source,
                        p,
                        "unexpected BOM at structural position",
                    ));
                }
                while p < b.len() && !delim(b[p]) {
                    p += 1;
                }
                Kind::Atom(&source[offset..p])
            }
        };
        tokens.push(Token { kind, offset });
    }
    tokens.push(Token {
        kind: Kind::End,
        offset: p,
    });
    Ok(tokens)
}

// Starter classification only; the lexer has already excluded token delimiters.
pub(crate) fn bare(s: &str) -> bool {
    let b = s.as_bytes();
    match b.first() {
        None | Some(b'+') => false,
        Some(b'-' | b'.') => {
            b.len() == 1 || b[0] == b[1] || !matches!(b[1], b'-' | b'+' | b'.' | b'0'..=b'9')
        }
        Some(b'0'..=b'9') => false,
        _ => true,
    }
}
fn special(s: &str) -> bool {
    let b = s.as_bytes();
    b.first() == Some(&b'+')
        || (b.len() > 1
            && matches!(b[0], b'-' | b'.')
            && b[0] != b[1]
            && matches!(b[1], b'-' | b'+' | b'.'))
}

struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token<'a>>,
    pos: usize,
}
impl<'a> Parser<'a> {
    fn peek(&self) -> &Kind<'a> {
        &self.tokens[self.pos].kind
    }
    fn next(&mut self) -> Kind<'a> {
        let kind = self.tokens[self.pos].kind.clone();
        if !matches!(kind, Kind::End) {
            self.pos += 1;
        }
        kind
    }
    fn err(&self, message: impl Into<String>) -> ParseError {
        ParseError::at(self.source, self.tokens[self.pos].offset, message)
    }
    fn expect(&mut self, kind: Kind<'a>) -> Result<(), ParseError> {
        if self.peek() != &kind {
            return Err(self.err(format!("expected {kind:?}")));
        }
        self.next();
        Ok(())
    }
    fn stopped(&self) -> bool {
        matches!(self.peek(), Kind::Close | Kind::End)
    }
    fn value_depth(&self, value: &Value, initial: usize) -> Result<(), ParseError> {
        if crate::value::within_depth(value, initial) {
            Ok(())
        } else {
            Err(self.err("composed value depth limit exceeded"))
        }
    }
    fn entry_depth(&self, entry: &ManyofEntry, initial: usize) -> Result<(), ParseError> {
        self.value_depth(
            match entry {
                ManyofEntry::Named(f) => &f.value,
                ManyofEntry::Anonymous(v) => v,
            },
            initial,
        )
    }
    fn name(&mut self) -> Result<String, ParseError> {
        match self.peek() {
            Kind::Atom(s) if bare(s) => {
                let result = s.to_string();
                self.next();
                Ok(result)
            }
            Kind::Quoted(s) if !s.is_empty() => {
                let result = s.clone();
                self.next();
                Ok(result)
            }
            _ => Err(self.err("expected a nonempty field name")),
        }
    }
    fn message(&mut self) -> Result<Vec<Field>, ParseError> {
        let mut fields = Vec::new();
        let mut names = HashSet::new();
        while !self.stopped() {
            self.expect(Kind::Open)?;
            let append = self.peek() == &Kind::Open
                && self
                    .tokens
                    .get(self.pos + 1)
                    .is_some_and(|t| t.kind == Kind::Atom("+."));
            if append {
                self.append(&mut fields)?;
            } else {
                let offset = self.tokens[self.pos].offset;
                let field = self.field_content()?;
                self.value_depth(&field.value, 1)?;
                if !names.insert(field.name.clone()) {
                    return Err(ParseError::at(
                        self.source,
                        offset,
                        format!("duplicate field {:?}", field.name),
                    ));
                }
                fields.push(field);
            }
            self.expect(Kind::Close)?;
        }
        Ok(fields)
    }
    fn field_content(&mut self) -> Result<Field, ParseError> {
        if self.peek() == &Kind::Open {
            self.next();
            let name = self.name()?;
            if self.peek() == &Kind::Close {
                self.next();
                let entries = self.list_items(true, None)?;
                return Ok(Field::new(name, Value::Many(entries.into())));
            }
            let alternative = self.name()?;
            self.expect(Kind::Close)?;
            let value = self.value()?;
            return Ok(Field::new(name, Lone::new(alternative, value)));
        }
        let name = self.name()?;
        Ok(Field::new(name, self.value()?))
    }
    fn value(&mut self) -> Result<Value, ParseError> {
        match self.peek() {
            Kind::Close => Ok(Value::Mesg(Default::default())),
            Kind::Open => Ok(Value::Mesg(self.message()?.into())),
            Kind::Dict => {
                self.next();
                Ok(Value::Dict(self.message()?.into()))
            }
            Kind::List => {
                self.next();
                self.long_list()
            }
            Kind::Nest => {
                self.next();
                Ok(Value::Nest(self.nest()?))
            }
            Kind::Empty | Kind::Quoted(_) | Kind::Multi(_) => {
                Ok(Value::String(self.string_body(true)?))
            }
            Kind::Atom(s) => {
                if *s == "+true" || *s == "+false" {
                    let value = *s == "+true";
                    self.next();
                    return Ok(Value::Bool(value));
                }
                if let Ok(n) = s.parse::<Number>() {
                    self.next();
                    return Ok(Value::Number(n));
                }
                if bare(s) {
                    return Ok(Value::String(self.string_body(false)?));
                }
                Err(self.err("reserved prefix is not a string starter"))
            }
            _ => Err(self.err("expected a field value")),
        }
    }
    fn string_body(&mut self, discriminated: bool) -> Result<String, ParseError> {
        let mut result = String::new();
        let mut unquoted = false;
        let mut count = 0;
        while !self.stopped() {
            match self.peek() {
                Kind::Quoted(s) | Kind::Multi(s) => {
                    result.push_str(s);
                    self.next();
                    unquoted = false;
                }
                Kind::Empty => {
                    self.next();
                    unquoted = false;
                }
                Kind::Atom(s) if count > 0 || discriminated || bare(s) => {
                    if unquoted {
                        result.push(' ');
                    }
                    result.push_str(s);
                    self.next();
                    unquoted = true;
                }
                _ => break,
            }
            count += 1;
        }
        if count == 0 {
            return Err(self.err("expected string content"));
        }
        Ok(result)
    }
    fn long_list(&mut self) -> Result<Value, ParseError> {
        let entries = self.list_items(false, None)?;
        if entries
            .first()
            .is_some_and(|e| matches!(e, ManyofEntry::Named(_)))
        {
            Ok(Value::Many(entries.into()))
        } else {
            let mut values = Vec::with_capacity(entries.len());
            for entry in entries {
                match entry {
                    ManyofEntry::Anonymous(value) => values.push(value),
                    ManyofEntry::Named(_) => {
                        return Err(self.err("array cannot contain named fields"));
                    }
                }
            }
            Ok(Value::List(values.into()))
        }
    }
    fn list_items(
        &mut self,
        force_many: bool,
        seed: Option<ElementKind>,
    ) -> Result<Vec<ManyofEntry>, ParseError> {
        let mut items = Vec::new();
        let mut many = force_many;
        let mut kind = seed;
        while !self.stopped() {
            let offset = self.tokens[self.pos].offset;
            let mut item = self.list_item(kind == Some(ElementKind::String))?;
            if items.is_empty() && !force_many && seed.is_none() {
                many = matches!(item, ManyofEntry::Named(_));
            }
            if let ManyofEntry::Anonymous(value) = &mut item {
                let incoming =
                    element_kind(value).ok_or_else(|| self.err("invalid anonymous list item"))?;
                match (kind, &mut *value) {
                    (None, _) => kind = Some(incoming),
                    (Some(ElementKind::String), Value::Number(n)) => {
                        *value = Value::String(n.as_str().into());
                    }
                    (Some(ElementKind::String), Value::Bool(b)) => {
                        *value = Value::String(if *b { "+true" } else { "+false" }.into());
                    }
                    (Some(ElementKind::Bool), Value::Number(n)) => {
                        let b = n.boolean_integer().ok_or_else(|| {
                            ParseError::at(
                                self.source,
                                offset,
                                "boolean list requires a boolean or nonnegative integer 0/1",
                            )
                        })?;
                        *value = Value::Bool(b);
                    }
                    (Some(k), _) if k == incoming => {}
                    _ => {
                        return Err(ParseError::at(
                            self.source,
                            offset,
                            "incompatible anonymous list element kind",
                        ));
                    }
                }
            } else if !many {
                return Err(ParseError::at(
                    self.source,
                    offset,
                    "array cannot contain named fields",
                ));
            }
            self.entry_depth(&item, 1)?;
            items.push(item);
        }
        Ok(items)
    }
    fn list_item(&mut self, string_first: bool) -> Result<ManyofEntry, ParseError> {
        let value = match self.peek() {
            Kind::Dict => {
                self.next();
                Value::Mesg(Default::default())
            }
            Kind::Open => {
                self.next();
                let value = match self.peek() {
                    Kind::Dict => {
                        self.next();
                        Value::Mesg(self.message()?.into())
                    }
                    Kind::Empty => {
                        self.next();
                        // Unlike a bare empty-string item, the parenthesized form needs content.
                        Value::String(self.string_body(true)?)
                    }
                    _ => {
                        let field = self.field_content()?;
                        self.expect(Kind::Close)?;
                        return Ok(ManyofEntry::Named(field));
                    }
                };
                self.expect(Kind::Close)?;
                value
            }
            Kind::Empty => {
                self.next();
                Value::String(String::new())
            }
            Kind::Quoted(s) | Kind::Multi(s) => {
                let s = s.clone();
                self.next();
                Value::String(s)
            }
            Kind::Atom(s) => {
                let s = *s;
                let value = if s == "+true" || s == "+false" {
                    Value::Bool(s == "+true")
                } else if let Ok(n) = s.parse() {
                    Value::Number(n)
                } else if bare(s) || string_first {
                    Value::String(s.into())
                } else {
                    return Err(self.err("reserved prefix is not a string starter"));
                };
                self.next();
                value
            }
            _ => return Err(self.err("expected a list element")),
        };
        Ok(ManyofEntry::Anonymous(value))
    }
    fn append(&mut self, fields: &mut [Field]) -> Result<(), ParseError> {
        self.expect(Kind::Open)?;
        self.expect(Kind::Atom("+."))?;
        let mut path = Vec::new();
        while !self.stopped() {
            path.push(self.name()?);
        }
        let Some((last, parents)) = path.split_last() else {
            return Err(self.err("append path must not be empty"));
        };
        self.expect(Kind::Close)?;
        self.expect(Kind::List)?;
        let mut current = fields;
        for component in parents {
            let target = current
                .iter_mut()
                .find(|f| &f.name == component)
                .ok_or_else(|| self.err("unknown append target"))?;
            current = match &mut target.value {
                Value::Mesg(f) => f.as_mut_slice(),
                Value::Dict(f) => f.as_mut_slice(),
                _ => return Err(self.err("append path must traverse messages or dicts")),
            };
        }
        let target = current
            .iter_mut()
            .find(|f| &f.name == last)
            .ok_or_else(|| self.err("unknown append target"))?;
        match &mut target.value {
            Value::List(values) => {
                let seed = values.as_slice().first().and_then(element_kind);
                let appended = self.list_items(false, seed)?;
                for item in appended {
                    match item {
                        ManyofEntry::Anonymous(value) => {
                            // Source depth and target-path depth compose here.
                            self.value_depth(&value, path.len() + 1)?;
                            values.push(value);
                        }
                        ManyofEntry::Named(_) => {
                            return Err(self.err("cannot append named fields to an array"));
                        }
                    }
                }
            }
            Value::Many(entries) => {
                let seed = entries.iter().find_map(|e| match e {
                    ManyofEntry::Anonymous(v) => element_kind(v),
                    _ => None,
                });
                let appended = self.list_items(true, seed)?;
                for entry in &appended {
                    self.entry_depth(entry, path.len() + 1)?;
                }
                entries.extend(appended);
            }
            _ => return Err(self.err("append target must be an array or manyof")),
        }
        Ok(())
    }
    fn nest(&mut self) -> Result<Nest, ParseError> {
        let mut items = Nest::new();
        while !self.stopped() {
            items.push(self.nest_item()?);
        }
        Ok(items)
    }
    fn nest_name(&mut self) -> Result<String, ParseError> {
        match self.peek() {
            Kind::Quoted(s) | Kind::Multi(s) if !s.is_empty() => {
                let s = s.clone();
                self.next();
                Ok(s)
            }
            Kind::Atom(s) if !special(s) => {
                let s = s.to_string();
                self.next();
                Ok(s)
            }
            _ => Err(self.err("expected a subnest name without a reserved prefix")),
        }
    }
    fn nest_item(&mut self) -> Result<NestItem, ParseError> {
        match self.peek() {
            Kind::Atom(s) => {
                let s = s.to_string();
                self.next();
                Ok(NestItem::Leaf(s))
            }
            Kind::Quoted(s) | Kind::Multi(s) => {
                let s = s.clone();
                self.next();
                Ok(NestItem::Leaf(s))
            }
            Kind::Empty => {
                self.next();
                Ok(NestItem::Leaf(String::new()))
            }
            Kind::Open => {
                self.next();
                if self.peek() == &Kind::Empty {
                    self.next();
                    if self.peek() == &Kind::Nest {
                        self.next();
                        let children = self.nest()?;
                        self.expect(Kind::Close)?;
                        return Ok(NestItem::Branch {
                            name: None,
                            children,
                        });
                    }
                    let text = self.string_body(true)?;
                    self.expect(Kind::Close)?;
                    return Ok(NestItem::Leaf(text));
                }
                if self.peek() == &Kind::Nest {
                    self.next();
                    let children = self.nest()?;
                    if children.is_empty() {
                        return Err(self.err("empty anonymous subnest requires a string discriminator before the nest discriminator"));
                    }
                    self.expect(Kind::Close)?;
                    return Ok(NestItem::Branch {
                        name: None,
                        children,
                    });
                }
                let name = self.nest_name()?;
                let children = if self.peek() == &Kind::Nest {
                    self.next();
                    self.nest()?
                } else if self.peek() == &Kind::Empty {
                    self.next();
                    vec![NestItem::Leaf(if self.peek() == &Kind::Close {
                        String::new()
                    } else {
                        self.string_body(true)?
                    })]
                    .into()
                } else {
                    self.nest()?
                };
                self.expect(Kind::Close)?;
                Ok(NestItem::Branch {
                    name: Some(name),
                    children,
                })
            }
            _ => Err(self.err("nest can only contain strings and subnests")),
        }
    }
}

/// Parse a complete UTF-8 SxPB document into precise owned values.
///
/// An empty document is an empty message. Trailing tokens are an error. The
/// basic escape set and [`MAX_DEPTH`] limit are described in the crate docs.
pub fn from_str(source: &str) -> Result<Value, ParseError> {
    let mut parser = Parser {
        source,
        tokens: tokenize(source)?,
        pos: 0,
    };
    let value = match parser.peek() {
        Kind::End => Value::Mesg(Default::default()),
        Kind::Dict => {
            parser.next();
            Value::Dict(parser.message()?.into())
        }
        Kind::List => {
            parser.next();
            parser.long_list()?
        }
        Kind::Nest => {
            parser.next();
            Value::Nest(parser.nest()?)
        }
        _ => Value::Mesg(parser.message()?.into()),
    };
    parser.expect(Kind::End)?;
    parser.value_depth(&value, 0)?;
    Ok(value)
}
