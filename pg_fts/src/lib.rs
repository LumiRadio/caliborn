//! Postgres full-text search expressions and schema helpers for sea-query.

mod config;
mod expr;
mod query;
mod schema;

pub use config::{FtsError, TsConfig};
pub use expr::FtsExprTrait;
pub use query::TsQuery;
pub use schema::{FtsColumn, gin_index, tsvector};
