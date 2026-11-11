use sqlx::SqlitePool;

use crate::KdsError;

/// Applique les migrations KDS sur le pool fourni.
/// Idempotent via `_applied_migrations` (créée par fiscal-engine).
///
/// # Errors
/// Retourne `KdsError::Database` si une requête SQL échoue.
pub async fn run_kds_migrations(pool: &SqlitePool) -> Result<(), KdsError> {
    let tracking_exists: bool = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='_applied_migrations'",
    )
    .fetch_one(pool)
    .await?
        > 0;

    apply_migration(
        pool,
        tracking_exists,
        "0008",
        include_str!("../migrations/0008_kds_schema.sql"),
    )
    .await?;

    apply_migration(
        pool,
        tracking_exists,
        "0010",
        include_str!("../migrations/0010_expo_role.sql"),
    )
    .await?;

    Ok(())
}

async fn apply_migration(
    pool: &SqlitePool,
    tracking_exists: bool,
    version: &str,
    sql: &str,
) -> Result<(), KdsError> {
    if tracking_exists {
        let already: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM _applied_migrations WHERE version = ?")
                .bind(version)
                .fetch_one(pool)
                .await?;
        if already > 0 {
            return Ok(());
        }
    }

    sqlx::query(sql).execute(pool).await?;

    if tracking_exists {
        sqlx::query("INSERT OR IGNORE INTO _applied_migrations (version) VALUES (?)")
            .bind(version)
            .execute(pool)
            .await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn in_memory_pool() -> SqlitePool {
        SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool")
    }

    #[tokio::test]
    async fn kds_migration_creates_tables() {
        let pool = in_memory_pool().await;

        sqlx::query("CREATE TABLE IF NOT EXISTS _applied_migrations (version TEXT PRIMARY KEY)")
            .execute(&pool)
            .await
            .unwrap();

        run_kds_migrations(&pool).await.expect("migration ok");

        // Idempotence
        run_kds_migrations(&pool).await.expect("idempotent");

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name LIKE 'kds_%'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(count >= 7, "expected at least 7 kds_ tables, got {count}");

        let v008: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM _applied_migrations WHERE version = '0008'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(v008, 1);

        let v010: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM _applied_migrations WHERE version = '0010'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(v010, 1);
    }

    #[tokio::test]
    async fn kds_migration_without_tracking_table() {
        let pool = in_memory_pool().await;

        run_kds_migrations(&pool)
            .await
            .expect("migration ok sans table de suivi");

        run_kds_migrations(&pool)
            .await
            .expect("idempotent sans table de suivi");

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name LIKE 'kds_%'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(count >= 7, "expected at least 7 kds_ tables, got {count}");
    }

    #[tokio::test]
    async fn expo_role_accepted_after_migration() {
        let pool = in_memory_pool().await;
        run_kds_migrations(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO kds_stations (id, name, role, output_type, active_in_profiles, sort_order, enabled)
             VALUES ('expo-01', 'Expo', 'expo', 'screen', '[\"normal\"]', 5, 1)",
        )
        .execute(&pool)
        .await
        .expect("expo station insert should succeed after 0010 migration");
    }
}
