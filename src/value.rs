//! Owned SxPB values. Containers retain their kind and source order.

use std::{
    fmt,
    num::{ParseFloatError, ParseIntError},
    str::FromStr,
};

/// Whether a numeric spelling is an integer or a decimal/exponent form.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum NumberKind {
    /// No decimal point or exponent.
    Integer,
    /// Contains a decimal point or exponent, even if mathematically integral.
    Float,
}

/// A validated, lossless numeric spelling of arbitrary size.
///
/// No arithmetic or rounding is performed. Equality is lexical: `1`, `+1`,
/// `01` and `1.0` are distinct. Conversion to machine types is explicit and
/// fallible. This also preserves signed zero and exponents outside f64's range.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Number {
    spelling: String,
    kind: NumberKind,
}

/// A string was not an SxPB numeric literal, or a float was not finite.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidNumber;

impl fmt::Display for InvalidNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("expected a finite SxPB numeric spelling")
    }
}
impl std::error::Error for InvalidNumber {}

impl Number {
    /// The original, validated numeric spelling.
    pub fn as_str(&self) -> &str {
        &self.spelling
    }
    /// Integer or floating-point syntax, independent of numeric magnitude.
    pub fn kind(&self) -> NumberKind {
        self.kind
    }
    /// Convert an integer spelling, failing for float syntax or overflow.
    pub fn to_i128(&self) -> Result<i128, ParseIntError> {
        self.spelling.parse()
    }
    /// Convert a nonnegative integer spelling, failing for float syntax or overflow.
    pub fn to_u128(&self) -> Result<u128, ParseIntError> {
        self.spelling.parse()
    }
    /// Convert to f64. This may round and may overflow to infinity or underflow
    /// to zero; it does not change the stored number.
    pub fn to_f64(&self) -> Result<f64, ParseFloatError> {
        self.spelling.parse()
    }

    pub(crate) fn boolean_integer(&self) -> Option<bool> {
        if self.kind != NumberKind::Integer || self.spelling.starts_with('-') {
            return None;
        }
        match self
            .spelling
            .trim_start_matches('+')
            .trim_start_matches('0')
        {
            "" => Some(false),
            "1" => Some(true),
            _ => None,
        }
    }
}

impl FromStr for Number {
    type Err = InvalidNumber;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = s.as_bytes();
        let mut i = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
        let mut digits = 0;
        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
            digits += 1;
        }
        let mut kind = NumberKind::Integer;
        if bytes.get(i) == Some(&b'.') {
            kind = NumberKind::Float;
            i += 1;
            while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
                digits += 1;
            }
        }
        if digits == 0 {
            return Err(InvalidNumber);
        }
        if matches!(bytes.get(i), Some(b'e' | b'E')) {
            kind = NumberKind::Float;
            i += 1;
            if matches!(bytes.get(i), Some(b'+' | b'-')) {
                i += 1;
            }
            let start = i;
            while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
            if start == i {
                return Err(InvalidNumber);
            }
        }
        if i != bytes.len() {
            return Err(InvalidNumber);
        }
        Ok(Self {
            spelling: s.into(),
            kind,
        })
    }
}
impl fmt::Display for Number {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.spelling)
    }
}
macro_rules! integer_from {
    ($($ty:ty),*) => {$(
        impl From<$ty> for Number {
            fn from(n: $ty) -> Self {
                Self { spelling: n.to_string(), kind: NumberKind::Integer }
            }
        }
        impl From<$ty> for Value {
            fn from(n: $ty) -> Self { Self::Number(n.into()) }
        }
    )*};
}
integer_from!(
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize
);

impl TryFrom<f64> for Number {
    type Error = InvalidNumber;
    fn try_from(n: f64) -> Result<Self, Self::Error> {
        if !n.is_finite() {
            return Err(InvalidNumber);
        }
        let mut spelling = n.to_string();
        if !spelling.contains(['.', 'e', 'E']) {
            spelling.push_str(".0");
        }
        Ok(Self {
            spelling,
            kind: NumberKind::Float,
        })
    }
}

// The wrappers are distinct types even where their storage is identical.
// Constructors intentionally defer semantic validation to serialization.
macro_rules! sequence_type {
    ($(#[$meta:meta])* $name:ident, $item:ty) => {
        $(#[$meta])*
        #[derive(Clone, Debug, Default, PartialEq, Eq)]
        pub struct $name(pub Vec<$item>);

        impl $name {
            /// Construct an empty container.
            pub fn new() -> Self { Self(Vec::new()) }
            /// Number of elements.
            pub fn len(&self) -> usize { self.0.len() }
            /// Whether the container is empty.
            pub fn is_empty(&self) -> bool { self.0.is_empty() }
            /// Append an element; semantic validation is deferred to serialization.
            pub fn push(&mut self, item: $item) { self.0.push(item); }
            /// View the ordered elements.
            pub fn as_slice(&self) -> &[$item] { &self.0 }
            /// Mutably view the ordered elements.
            pub fn as_mut_slice(&mut self) -> &mut [$item] { &mut self.0 }
            /// Iterate over the ordered elements.
            pub fn iter(&self) -> std::slice::Iter<'_, $item> { self.0.iter() }
            /// Mutably iterate over the ordered elements.
            pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, $item> { self.0.iter_mut() }
            /// Recover the underlying vector without copying or allocating.
            pub fn into_vec(self) -> Vec<$item> { self.0 }
        }
        impl From<Vec<$item>> for $name {
            fn from(items: Vec<$item>) -> Self { Self(items) }
        }
        impl FromIterator<$item> for $name {
            fn from_iter<T: IntoIterator<Item = $item>>(items: T) -> Self { Self(items.into_iter().collect()) }
        }
        impl Extend<$item> for $name {
            fn extend<T: IntoIterator<Item = $item>>(&mut self, items: T) { self.0.extend(items); }
        }
        impl IntoIterator for $name {
            type Item = $item;
            type IntoIter = std::vec::IntoIter<$item>;
            fn into_iter(self) -> Self::IntoIter { self.0.into_iter() }
        }
        impl<'a> IntoIterator for &'a $name {
            type Item = &'a $item;
            type IntoIter = std::slice::Iter<'a, $item>;
            fn into_iter(self) -> Self::IntoIter { self.iter() }
        }
        impl<'a> IntoIterator for &'a mut $name {
            type Item = &'a mut $item;
            type IntoIter = std::slice::IterMut<'a, $item>;
            fn into_iter(self) -> Self::IntoIter { self.iter_mut() }
        }
        impl From<$name> for Value {
            fn from(items: $name) -> Self { Self::$name(items) }
        }
    };
}

sequence_type!(
    /// Ordered message fields without a dict discriminator. Names must be nonempty and unique.
    Mesg, Field
);
sequence_type!(
    /// Ordered dict fields with the `()` discriminator. Distinct from [`Mesg`].
    Dict, Field
);
sequence_type!(
    /// Homogeneous anonymous scalar or message elements.
    List, Value
);
sequence_type!(
    /// Ordered named and anonymous alternatives. Named alternatives may repeat.
    Many, ManyofEntry
);
sequence_type!(
    /// An ordered string tree, not a map or an arbitrary list of values.
    Nest, NestItem
);

/// Exactly one named alternative. Validation is deferred to serialization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lone {
    /// The alternative's nonempty name.
    pub name: String,
    /// The alternative's payload.
    pub value: Box<Value>,
}
impl Lone {
    /// Construct a named alternative.
    pub fn new(name: impl Into<String>, value: impl Into<Value>) -> Self {
        Self {
            name: name.into(),
            value: Box::new(value.into()),
        }
    }
}
impl From<Lone> for Value {
    fn from(lone: Lone) -> Self {
        Self::Lone(lone)
    }
}

macro_rules! field_access {
    ($name:ident) => {
        impl $name {
            /// Look up a field by name.
            pub fn get(&self, name: &str) -> Option<&Value> {
                self.iter().find(|f| f.name == name).map(|f| &f.value)
            }
            /// Mutably look up a field by name.
            pub fn get_mut(&mut self, name: &str) -> Option<&mut Value> {
                self.iter_mut()
                    .find(|f| f.name == name)
                    .map(|f| &mut f.value)
            }
        }
    };
}
field_access!(Mesg);
field_access!(Dict);

/// An explicitly named field, including a loneof or manyof alternative.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// A nonempty field name. Serialization validates this requirement.
    pub name: String,
    /// The field's value.
    pub value: Value,
}
impl Field {
    /// Construct a field. Validation is deferred to serialization.
    pub fn new(name: impl Into<String>, value: impl Into<Value>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

/// A manyof alternative. An anonymous value is not a field named `value`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManyofEntry {
    /// An explicit alternative; names may repeat within a manyof.
    Named(Field),
    /// A scalar or message with no explicit name.
    Anonymous(Value),
}

/// A nest contains string leaves and branches, not arbitrary SxPB values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NestItem {
    /// A string leaf, including an empty string.
    Leaf(String),
    /// A branch; `None` is anonymous, `Some` must contain a nonempty name.
    Branch {
        /// The optional branch label.
        name: Option<String>,
        /// Ordered children. Empty branches are allowed.
        children: Nest,
    },
}

/// A precise, owned SxPB value.
///
/// Programmatically built values are validated by [`crate::to_string`]. Not
/// every tree has a faithful SxPB spelling in every context: e.g. scalars are
/// not document roots and arrays cannot contain arbitrary container kinds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// UTF-8 string.
    String(String),
    /// Boolean (`+true` or `+false`).
    Bool(bool),
    /// Lossless integer or float spelling.
    Number(Number),
    /// Ordered, uniquely named fields, with no dict discriminator.
    Mesg(Mesg),
    /// Ordered, uniquely named fields, with the `()` discriminator.
    Dict(Dict),
    /// Homogeneous anonymous elements.
    List(List),
    /// Exactly one named alternative.
    Lone(Lone),
    /// Ordered named and anonymous alternatives.
    Many(Many),
    /// Ordered string tree.
    Nest(Nest),
}
impl Value {
    /// Look up a message/dict field by name.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.as_fields()?
            .iter()
            .find(|f| f.name == name)
            .map(|f| &f.value)
    }
    /// Mutably look up a message/dict field by name.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Value> {
        match self {
            Self::Mesg(fields) => fields.get_mut(name),
            Self::Dict(fields) => fields.get_mut(name),
            _ => None,
        }
    }
    /// View a message or dict's ordered fields.
    pub fn as_fields(&self) -> Option<&[Field]> {
        match self {
            Self::Mesg(f) => Some(f.as_slice()),
            Self::Dict(f) => Some(f.as_slice()),
            _ => None,
        }
    }
    /// View a string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }
    /// Read a boolean.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }
    /// View a lossless number.
    pub fn as_number(&self) -> Option<&Number> {
        match self {
            Self::Number(n) => Some(n),
            _ => None,
        }
    }
    /// View an array's elements.
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Self::List(v) => Some(v.as_slice()),
            _ => None,
        }
    }
}
macro_rules! container_access {
    ($name:ident, $get:ident, $get_mut:ident) => {
        impl Value {
            #[doc = concat!("Borrow the contained [", stringify!($name), "], if this is that kind.")]
            pub fn $get(&self) -> Option<&$name> {
                match self { Self::$name(items) => Some(items), _ => None }
            }
            #[doc = concat!("Mutably borrow the contained [", stringify!($name), "], if this is that kind.")]
            pub fn $get_mut(&mut self) -> Option<&mut $name> {
                match self { Self::$name(items) => Some(items), _ => None }
            }
        }
    };
}
container_access!(Mesg, as_mesg, as_mesg_mut);
container_access!(Dict, as_dict, as_dict_mut);
container_access!(List, as_list, as_list_mut);
container_access!(Lone, as_lone, as_lone_mut);
container_access!(Many, as_many, as_many_mut);
container_access!(Nest, as_nest, as_nest_mut);

impl From<String> for Value {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}
impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Self::String(s.into())
    }
}
impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Self::Bool(b)
    }
}
impl From<Number> for Value {
    fn from(n: Number) -> Self {
        Self::Number(n)
    }
}
impl FromStr for Value {
    type Err = crate::ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        crate::from_str(s)
    }
}

// Iterative so validating a caller-built tree does not itself recurse. Parser
// attachment checks use an initial depth relative to the enclosing message.
pub(crate) fn within_depth(value: &Value, initial: usize) -> bool {
    enum Node<'a> {
        Value(&'a Value, usize),
        Nest(&'a NestItem, usize),
    }
    let mut pending = vec![Node::Value(value, initial)];
    while let Some(node) = pending.pop() {
        let depth = match &node {
            Node::Value(_, depth) | Node::Nest(_, depth) => *depth,
        };
        if depth > crate::MAX_DEPTH {
            return false;
        }
        match node {
            Node::Value(value, _) => match value {
                Value::Mesg(fields) => {
                    pending.extend(fields.iter().map(|f| Node::Value(&f.value, depth + 1)));
                }
                Value::Dict(fields) => {
                    pending.extend(fields.iter().map(|f| Node::Value(&f.value, depth + 1)));
                }
                Value::List(values) => {
                    pending.extend(values.iter().map(|v| Node::Value(v, depth + 1)))
                }
                Value::Lone(lone) => pending.push(Node::Value(&lone.value, depth + 1)),
                Value::Many(entries) => pending.extend(entries.iter().map(|e| {
                    Node::Value(
                        match e {
                            ManyofEntry::Named(f) => &f.value,
                            ManyofEntry::Anonymous(v) => v,
                        },
                        depth + 1,
                    )
                })),
                Value::Nest(items) => {
                    pending.extend(items.iter().map(|item| Node::Nest(item, depth + 1)))
                }
                _ => {}
            },
            Node::Nest(NestItem::Branch { children, .. }, _) => {
                pending.extend(children.iter().map(|item| Node::Nest(item, depth + 1)))
            }
            Node::Nest(NestItem::Leaf(_), _) => {}
        }
    }
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ElementKind {
    String,
    Bool,
    Number,
    Message,
}
pub(crate) fn element_kind(value: &Value) -> Option<ElementKind> {
    match value {
        Value::String(_) => Some(ElementKind::String),
        Value::Bool(_) => Some(ElementKind::Bool),
        Value::Number(_) => Some(ElementKind::Number),
        Value::Mesg(_) => Some(ElementKind::Message),
        _ => None,
    }
}
