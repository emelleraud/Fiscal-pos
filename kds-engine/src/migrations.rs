use sqlx::SqlitePool;

use crate::KdsError;

/// Applique les migrations KDS sur le pool fourni.
/// Idempotent : ignore les migrations déjà appliquées (via `_applied_migrations`).
///
/// La table `_applied_migrations` est créée par `fiscal-engine` (`JournalStore::new`).
/// Si elle n'existe pas encore, cette fonction l'ignore et applique directement
/// le schéma KDS sans enregistrement de version.
///
/// # Errors
/// Retourne `KdsError::Database` si une requête SQL échoue.
pub async fn run_kds_migrations(pool: &SqlitePool) -> Result<(), KdsError> {
    // Vérifier que _applied_migrations existe (elle est créée par fiscal-engine).
    let table_exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='_applied_migrations'",
    )
    .fetch_one(pool)
    .await?;

    if table_exists > 0 {
        // La table de suivi existe — vérifier si 0008 est déjà appliquée.
        let already_applied: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM _applied_migrations WHERE version = '0008'")
                .fetch_one(pool)
                .await?;

        if already_applied > 0 {
            return Ok(());
        }
    }

    // Appliquer le schéma KDS.
    sqlx::query(include_str!("../migrations/0008_kds_schema.sql"))
        .execute(pool)
        .await?;

    // Enregistrer la version si la table de suivi existe.
    if table_exists > 0 {
        sqlx::query("INSERT OR IGNORE INTO _applied_migrations (version) VALUES ('0008')")
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

        // Simuler le bootstrap fait par fiscal-engine : créer _applied_migrations.
        sqlx::query("CREATE TABLE IF NOT EXISTS _applied_migrations (version TEXT PRIMARY KEY)")
            .execute(&pool)
            .await
            .unwrap();

        run_kds_migrations(&pool).await.expect("migration ok");

        // Idempotence : un second appel ne doit pas échouer.
        run_kds_migrations(&pool).await.expect("idempotent");

        // Vérifier que les tables KDS sont créées.
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name LIKE 'kds_%'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(count >= 7, "expected at least 7 kds_ tables, got {count}");

        // Vérifier que la version est enregistrée.
        let version: String =
            sqlx::query_scalar("SELECT version FROM _applied_migrations WHERE version = '0008'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(version, "0008");
    }

    #[tokio::test]
    async fn kds_migration_without_tracking_table() {
        // Sans _applied_migrations (pas encore créée par fiscal-engine).
        let pool = in_memory_pool().await;

        run_kds_migrations(&pool)
            .await
            .expect("migration ok sans table de suivi");

        // Les tables KDS doivent être créées quand même.
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name LIKE 'kds_%'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(count >= 7, "expected at least 7 kds_ tables, got {count}");
    }
}
