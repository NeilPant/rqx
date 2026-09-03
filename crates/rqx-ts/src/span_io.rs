//! `oxc::span::Span` carries no serde impls, and enabling oxc's `serialize` feature
//! to get them would pull in its whole ESTree layer for two integers.

use oxc::span::Span;
use serde::{Deserialize, Deserializer, Serializer};

pub fn serialize<S: Serializer>(span: &Span, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_newtype_struct("Span", &(span.start, span.end))
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Span, D::Error> {
    let (start, end) = <(u32, u32)>::deserialize(deserializer)?;
    Ok(Span::new(start, end))
}
