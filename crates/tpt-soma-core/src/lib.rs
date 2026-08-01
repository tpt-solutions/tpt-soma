pub mod connection;
pub mod migrations;
pub mod query;
pub mod store;
pub mod dp;

pub use connection::{CoreError, PgPool, create_pool, run_migrations};

pub type Error = CoreError;
pub type Result<T> = std::result::Result<T, Error>;