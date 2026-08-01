use sqlx::migrate::{MigrateError, Migrator};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migration(#[from] MigrateError),
    #[error("connection pool error: {0}")]
    Pool(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;

pub type PgPool = sqlx::PgPool;

pub async fn create_pool(database_url: &str) -> Result<PgPool> {
    let pool = PgPool::connect(database_url).await?;
    Ok(pool)
}

pub async fn run_migrations(pool: &PgPool) -> Result<()> {
    let migrator = Migrator::new(std::path::Path::new("./migrations")).await?;
    migrator.run(pool).await?;
    Ok(())
}