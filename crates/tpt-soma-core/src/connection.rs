use sqlx::migrate::MigrateError;
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

/// Applies the migrations embedded at compile time. Reading them from disk
/// relative to the working directory failed in the container, which runs with
/// no `WORKDIR` in its runtime stage.
pub async fn run_migrations(pool: &PgPool) -> Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}
