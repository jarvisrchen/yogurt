//! Schema and REST client for Yogurt Cloud, the user's own Supabase project.

pub mod config;
mod error;
pub mod push;
mod rest;

pub use error::{Error, Result};
pub use rest::{Project, Session};

pub const SCHEMA_SQL: &str = include_str!("../schema.sql");
pub const OWNER_SQL: &str = include_str!("../owner.sql");
