use sqlx::{postgres::PgPoolOptions, PgPool};

pub async fn connect(database_url: &str) -> anyhow::Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .min_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(8))
        .connect(database_url)
        .await?;
    let mut migrator = sqlx::migrate!("./migrations");
    // The first production deployment contained an uncommitted v3 migration.
    // Keep its database state and allow the tracked v4 migration to continue.
    migrator.set_ignore_missing(true);
    migrator.run(&pool).await?;
    Ok(pool)
}
