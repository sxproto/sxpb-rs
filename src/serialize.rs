use std::collections::HashSet;

use crate::value::{ElementKind, element_kind};
use crate::{Field, MAX_DEPTH, ManyofEntry, NestItem, SerializeError, Value};

type Result<T> = std::result::Result<T, SerializeError>;

fn bare_atom(text: &str) -> bool {
    crate::parse::bare(text)
        && !text.chars().any(|ch| {
            ch.is_whitespace()
                || ch.is_control()
                || matches!(ch, '(' | ')' | ';' | '"' | '\u{feff}')
        })
}

struct Writer {
    output: String,
    depth: usize,
}
impl Writer {
    fn open(&mut self) -> Result<()> {
        if self.depth >= MAX_DEPTH {
            return Err(SerializeError::new("nesting depth limit exceeded"));
        }
        self.depth += 1;
        self.output.push('(');
        Ok(())
    }
    fn close(&mut self) {
        self.depth -= 1;
        self.output.push(')');
    }
    fn space(&mut self) {
        self.output.push(' ');
    }
    fn separator(&mut self) {
        self.output.push(if self.depth == 0 { '\n' } else { ' ' });
    }
    fn marker(&mut self, text: &str, depth: usize) -> Result<()> {
        if self.depth + depth > MAX_DEPTH {
            return Err(SerializeError::new("nesting depth limit exceeded"));
        }
        self.output.push_str(text);
        Ok(())
    }
    fn quoted(&mut self, s: &str) {
        self.output.push('"');
        for ch in s.chars() {
            match ch {
                '"' => self.output.push_str("\\\""),
                '\\' => self.output.push_str("\\\\"),
                '\t' => self.output.push_str("\\t"),
                '\n' => self.output.push_str("\\n"),
                '\r' => self.output.push_str("\\r"),
                '\u{b}' => self.output.push_str("\\v"),
                '\u{c}' => self.output.push_str("\\f"),
                // Other control characters have no basic escape; raw content is legal.
                _ => self.output.push(ch),
            }
        }
        self.output.push('"');
    }
    fn atom(&mut self, text: &str) {
        if bare_atom(text) {
            self.output.push_str(text);
        } else {
            self.quoted(text);
        }
    }
    fn name(&mut self, name: &str) -> Result<()> {
        if name.is_empty() {
            return Err(SerializeError::new(
                "field and named branch names must not be empty",
            ));
        }
        self.atom(name);
        Ok(())
    }
    fn fields(&mut self, fields: &[Field]) -> Result<()> {
        let mut names = HashSet::new();
        for (i, field) in fields.iter().enumerate() {
            if !names.insert(&field.name) {
                return Err(SerializeError::new(format!(
                    "duplicate field {:?}",
                    field.name
                )));
            }
            if i > 0 {
                self.separator();
            }
            self.field(field)?;
        }
        Ok(())
    }
    fn field(&mut self, field: &Field) -> Result<()> {
        self.open()?;
        match &field.value {
            Value::Many(entries) => {
                self.open()?;
                self.name(&field.name)?;
                self.close();
                self.many_items(entries.as_slice())?;
            }
            Value::Lone(crate::Lone { name, value }) => {
                self.open()?;
                self.name(&field.name)?;
                self.space();
                self.name(name)?;
                self.close();
                if !matches!(value.as_ref(), Value::Mesg(f) if f.is_empty()) {
                    self.space();
                    self.body(value)?;
                }
            }
            value => {
                self.name(&field.name)?;
                if !matches!(value, Value::Mesg(f) if f.is_empty()) {
                    self.space();
                    self.body(value)?;
                }
            }
        }
        self.close();
        Ok(())
    }
    fn body(&mut self, value: &Value) -> Result<()> {
        match value {
            Value::String(s) => self.atom(s),
            Value::Bool(b) => self.output.push_str(if *b { "+true" } else { "+false" }),
            Value::Number(n) => self.output.push_str(n.as_str()),
            Value::Mesg(fields) => self.fields(fields.as_slice())?,
            Value::Dict(fields) => {
                self.marker("()", 1)?;
                if !fields.is_empty() {
                    self.separator();
                    self.fields(fields.as_slice())?;
                }
            }
            Value::List(values) => {
                self.marker("(())", 2)?;
                let mut kind = None;
                for value in values {
                    same_kind(&mut kind, value)?;
                    self.separator();
                    self.anonymous(value)?;
                }
            }
            Value::Many(entries) => {
                if !matches!(entries.as_slice().first(), Some(ManyofEntry::Named(_))) {
                    return Err(SerializeError::new(
                        "a root or loneof-payload manyof must begin with a named entry; wrap it in a named field otherwise",
                    ));
                }
                self.marker("(())", 2)?;
                self.many_items(entries.as_slice())?;
            }
            Value::Nest(items) => {
                self.marker("(\"\")", 1)?;
                self.nest_items(items.as_slice())?;
            }
            Value::Lone(_) => {
                return Err(SerializeError::new(
                    "a loneof needs a containing field name",
                ));
            }
        }
        Ok(())
    }
    fn anonymous(&mut self, value: &Value) -> Result<()> {
        match value {
            Value::Mesg(fields) => {
                if fields.is_empty() {
                    self.marker("()", 1)?;
                } else {
                    self.open()?;
                    self.marker("()", 1)?;
                    self.space();
                    self.fields(fields.as_slice())?;
                    self.close();
                }
            }
            Value::String(_) | Value::Bool(_) | Value::Number(_) => self.body(value)?,
            _ => {
                return Err(SerializeError::new(
                    "anonymous list elements must be scalars or messages (not dicts or other containers)",
                ));
            }
        }
        Ok(())
    }
    fn many_items(&mut self, entries: &[ManyofEntry]) -> Result<()> {
        let mut kind = None;
        for entry in entries {
            self.separator();
            match entry {
                ManyofEntry::Named(field) => self.field(field)?,
                ManyofEntry::Anonymous(value) => {
                    same_kind(&mut kind, value)?;
                    self.anonymous(value)?;
                }
            }
        }
        Ok(())
    }
    fn nest_items(&mut self, items: &[NestItem]) -> Result<()> {
        for item in items {
            self.separator();
            match item {
                NestItem::Leaf(s) => self.atom(s),
                NestItem::Branch { name, children } => {
                    self.open()?;
                    match name {
                        Some(name) => self.name(name)?,
                        None => self.quoted(""),
                    }
                    self.space();
                    self.marker("(\"\")", 1)?;
                    self.nest_items(children.as_slice())?;
                    self.close();
                }
            }
        }
        Ok(())
    }
}

fn same_kind(kind: &mut Option<ElementKind>, value: &Value) -> Result<()> {
    let incoming = element_kind(value)
        .ok_or_else(|| SerializeError::new("anonymous elements must be scalars or messages"))?;
    match *kind {
        None => *kind = Some(incoming),
        Some(k) if k == incoming => {}
        _ => {
            return Err(SerializeError::new(
                "incompatible anonymous list element kinds; normalize caller-built values explicitly",
            ));
        }
    }
    Ok(())
}

/// Serialize a precise SxPB document, rejecting trees with no faithful spelling.
///
/// Output is deterministic: document-level items occupy separate lines,
/// nested bodies stay compact, and names/strings use bare atoms only when
/// unambiguous. There is no terminal newline. This preserves data and numeric
/// spellings, not comments or source layout. Caller-built
/// lists must already be homogeneous; serialization never coerces their values.
/// Scalars and loneofs cannot be document roots. A root manyof must begin with
/// a named entry. Syntactic and composed-value depth are both limited by
/// [`MAX_DEPTH`]. Canonical discriminators may make a boundary-depth input's
/// output too deep; that is reported as an error, not a lossy rewrite.
pub fn to_string(value: &Value) -> Result<String> {
    if !crate::value::within_depth(value, 0) {
        return Err(SerializeError::new("composed value depth limit exceeded"));
    }
    if matches!(
        value,
        Value::String(_) | Value::Bool(_) | Value::Number(_) | Value::Lone(_)
    ) {
        return Err(SerializeError::new(
            "document root must be a message, dict, array, manyof or nest",
        ));
    }
    let mut writer = Writer {
        output: String::new(),
        depth: 0,
    };
    writer.body(value)?;
    Ok(writer.output)
}
