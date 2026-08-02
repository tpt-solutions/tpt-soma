#[cfg(test)]
pub mod test_helpers {
    use sqlx::PgPool;

    pub async fn test_pool() -> Result<PgPool, sqlx::Error> {
        let database_url =
            std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/tpt_soma_test".to_string());
        sqlx::PgPool::connect(&database_url).await
    }
}
