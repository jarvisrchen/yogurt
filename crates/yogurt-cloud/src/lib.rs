//! Encryption, link code, and schema for Yogurt Cloud. The laptop build can only encrypt.

mod crypto;
mod error;
mod link;

pub use crypto::{generate_keypair, seal, wrap_private_key, KeyPair};
pub use error::{Error, Result};
pub use link::LinkCode;

pub const SCHEMA_SQL: &str = include_str!("../schema.sql");

#[cfg(test)]
mod fixtures;
