use sqlx::PgPool;

/// Database for DB-backed tests. Required, with no default, because these tests
/// delete rows: a fallback to a developer's local server could wipe real data.
pub fn test_database_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .expect("set TEST_DATABASE_URL to a disposable database; the DB-backed tests delete rows")
}

pub async fn test_pool() -> Result<PgPool, sqlx::Error> {
    let pool = sqlx::PgPool::connect(&test_database_url()).await?;
    // TEST_SKIP_MIGRATIONS lets the audit tests run on plain Postgres with only
    // the audit table applied. Keystone-backed runs leave it unset.
    if std::env::var_os("TEST_SKIP_MIGRATIONS").is_none() {
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|e| sqlx::Error::Migrate(Box::new(e)))?;
    }
    Ok(pool)
}
