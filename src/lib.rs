//! A schema-agnostic, precise SxPB parser and serializer.
//!
//! ```
//! use sxpb::{from_str, to_string, Value};
//!
//! let data = from_str("(enabled +true) (count 123456789012345678901234567890)")?;
//! assert_eq!(data.get("enabled").and_then(Value::as_bool), Some(true));
//! let count = data.get("count").and_then(Value::as_number).unwrap();
//! assert_eq!(count.as_str(), "123456789012345678901234567890");
//! assert_eq!(from_str(&to_string(&data)?)?, data);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Containers are also standalone types, usable without matching on [`Value`]:
//!
//! ```
//! use sxpb::{Nest, NestItem, Value};
//!
//! fn inspect(nest: &Nest) -> usize { nest.len() }
//! let nest = Nest::from(vec![NestItem::Leaf("hello".into())]);
//! assert_eq!(inspect(&nest), 1);
//! let value: Value = nest.into();
//! assert_eq!(value.as_nest().map(inspect), Some(1));
//! ```
//!
//! The same pattern applies to [`Mesg`], [`Dict`], [`List`], [`Lone`] and [`Many`].
//! Wrapping a container in [`Value`] does not require an additional allocation.
//!
//! Values retain message/dict, array/manyof and loneof/nest distinctions, field
//! order, anonymous entry identity, and numeric spelling. [`Number`] stores
//! arbitrary-size validated numeric text rather than silently rounding it.
//!
//! The accepted quoted-string escapes are `\"`, `\\`, `\t`, `\n`, `\v`,
//! `\f`, `\r`, and LF/CRLF line continuations. JSON-only `\u`, `\b`, `\/`
//! and unknown escapes are rejected. Raw CR inside quoted strings is dropped;
//! escaped CR is retained. Triple-quoted content is not dedented. Other raw
//! control characters and Unicode scalar values are legal string content.
//!
//! Only ASCII SxPB blank space is structural whitespace. A structural BOM is
//! rejected; one inside an atom or quoted string remains content. Discriminator
//! internals allow blank space, not comments. These are this implementation's
//! choices where existing implementations differ, not new language rulings.
//!
//! This crate uses no unsafe code and no runtime dependencies. The fixed
//! [`MAX_DEPTH`] bounds both syntactic parentheses and composed-value nesting
//! during parsing and serialization. Append attachment is checked before it
//! can build an oversized tree; input/output size is otherwise limited by
//! available memory.
//! Arbitrarily deep caller-built trees can still overflow the stack when
//! cloned, compared, formatted with Debug, or dropped: this is not a universal
//! panic-freedom guarantee.

#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]
#![warn(missing_docs)]

mod error;
mod parse;
mod serialize;
mod value;

pub use error::{ParseError, SerializeError};
pub use parse::from_str;
pub use serialize::to_string;
pub use value::{
    Dict, Field, InvalidNumber, List, Lone, Many, ManyofEntry, Mesg, Nest, NestItem, Number,
    NumberKind, Value,
};

/// Maximum syntactic parenthesis nesting and composed-value depth.
///
/// Syntax counts discriminator parentheses too. Composed trees count edges
/// from the root value (depth zero), including each nest leaf/branch; named and
/// anonymous list entries each add one edge to their value. Both bounds are
/// checked because explicit append can compose deeper trees than source syntax.
/// This is an implementation resource limit, not a SxPB grammar rule.
pub const MAX_DEPTH: usize = 256;
