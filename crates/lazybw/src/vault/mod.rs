//! The vault model: cache, filters, folders, search, health and duplicates.
//!
//! Nothing here talks to `bw`; it is pure data derived from the models in
//! [`crate::bw::models`]. This keeps search, ranking, tree building and health
//! scoring testable without a terminal or a backend.

pub mod cache;
pub mod dupes;
pub mod filter;
pub mod folders;
pub mod folds;
pub mod health;
pub mod query;

pub use cache::{Filter, Row, VaultCache};
pub use health::{HealthCategory, HealthFlags, HealthReport};
