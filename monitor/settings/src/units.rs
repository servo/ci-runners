use std::iter::Sum;
use std::ops::Mul;
use std::str::FromStr;

use bytesize::ByteSize;
use jane_eyre::eyre::{self, Context, bail};
use serde::Deserialize;
use serde::de::Visitor;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct MemorySize(ByteSize);

impl FromStr for MemorySize {
    type Err = eyre::Report;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let parse_u64_maybe_space = |input: &str| -> Result<u64, Self::Err> {
            let input = input.strip_suffix(" ").unwrap_or(input);
            Ok(input.parse().wrap_err("bad number format")?)
        };
        if let Some(input) = input.strip_suffix("B") {
            Ok(MemorySize(ByteSize::b(parse_u64_maybe_space(input)?)))
        } else if let Some(input) = input.strip_suffix("K") {
            Ok(MemorySize(ByteSize::kib(parse_u64_maybe_space(input)?)))
        } else if let Some(input) = input.strip_suffix("M") {
            Ok(MemorySize(ByteSize::kib(parse_u64_maybe_space(input)?)))
        } else if let Some(input) = input.strip_suffix("G") {
            Ok(MemorySize(ByteSize::kib(parse_u64_maybe_space(input)?)))
        } else if let Some(input) = input.strip_suffix("T") {
            Ok(MemorySize(ByteSize::kib(parse_u64_maybe_space(input)?)))
        } else if let Some(input) = input.strip_suffix("P") {
            Ok(MemorySize(ByteSize::kib(parse_u64_maybe_space(input)?)))
        } else {
            bail!("bad format")
        }
    }
}

impl<'de> Deserialize<'de> for MemorySize {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(MemorySizeVisitor)
    }
}

pub(crate) struct MemorySizeVisitor;

impl<'de> Visitor<'de> for MemorySizeVisitor {
    type Value = MemorySize;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("[0-9]+ ?[BKMGTP]")
    }

    fn visit_str<E>(self, input: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        input
            .parse::<Self::Value>()
            .map_err(|e| E::custom(e.to_string()))
    }
}

impl Mul<MemorySize> for usize {
    type Output = MemorySize;

    fn mul(self, rhs: MemorySize) -> Self::Output {
        const _STATIC_ASSERT: () = assert!(size_of::<usize>() <= size_of::<u64>());
        MemorySize(self as u64 * rhs.0)
    }
}

impl Sum for MemorySize {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        Self(ByteSize::b(iter.map(|size| size.0.as_u64()).sum()))
    }
}
