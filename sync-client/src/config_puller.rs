//! # `config_puller`
//!
//! Réception et application de la configuration cloud (menu, prix, promotions).
//!
//! ## Flux
//! 1. Interroger `site_configs` sur Supabase (GET)
//! 2. Comparer la `version` avec la version locale (lue depuis `menu.json`)
//! 3. Si nouvelle version : écrire `menu.json` sur disque
//! 4. L'`edge-api` lira automatiquement la nouvelle carte au prochain appel GET /menu
//!
//! ## Résilience
//! Si le fichier `menu.json` ne peut pas être écrit (permissions, disque plein),
//! l'erreur est loggée mais non fatale — la configuration précédente reste active.
//!
//! ## Atomicité de l'écriture
//! L'écriture se fait en deux temps : écriture dans `menu.json.tmp` puis
//! renommage atomique (`rename()`) pour éviter qu'un crash laisse un fichier
//! corrompu à moitié écrit.

use tracing::{debug, info, warn};

use crate::{
    client::SupabaseClient,
    config::SyncConfig,
    error::SyncError,
    serializer::RemoteConfig,
};

/// Vérifie si une nouvelle configuration est disponible et l'applique.
///
/// Retourne `true` si la configuration a été mise à jour, `false` sinon.
///
/// # Errors
/// - `SyncError::Network` si Supabase est inaccessible
/// - Les erreurs d'écriture disque sont loggées mais non retournées (non fatales)
pub async fn pull_and_apply_config(
    client: &SupabaseClient,
    config: &SyncConfig,
) -> Result<bool, SyncError> {
    let Some(remote) = client.pull_config(&config.site_id).await? else {
        debug!("Aucune configuration disponible pour le site {}", config.site_id);
        return Ok(false);
    };

    // Vérifier si c'est une nouvelle version
    let local_version = read_local_version(&config.data_dir);

    if remote.version <= local_version {
        debug!(
            remote_version = remote.version,
            local_version = local_version,
            "Configuration à jour, pas de mise à jour nécessaire"
        );
        return Ok(false);
    }

    info!(
        remote_version = remote.version,
        local_version = local_version,
        site_id = %config.site_id,
        "Nouvelle configuration disponible — application en cours"
    );

    // Appliquer la nouvelle configuration
    apply_config(&remote, &config.data_dir);

    Ok(true)
}

/// Lit la version de la configuration locale depuis `menu.json`.
///
/// Retourne 0 si le fichier est absent ou invalide.
fn read_local_version(data_dir: &str) -> u32 {
    let path = format!("{data_dir}/menu.json");
    let Ok(content) = std::fs::read_to_string(&path) else { return 0 };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else { return 0 };
    value
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .map_or(0, |v| u32::try_from(v).unwrap_or(0))
}

/// Écrit la nouvelle configuration sur disque de manière atomique.
///
/// Utilise le pattern write-then-rename pour éviter les fichiers corrompus.
fn apply_config(remote: &RemoteConfig, data_dir: &str) {
    // S'assurer que le répertoire de données existe
    if let Err(e) = std::fs::create_dir_all(data_dir) {
        warn!(error = %e, data_dir = %data_dir, "Impossible de créer le répertoire data");
        return;
    }

    let menu_path = format!("{data_dir}/menu.json");
    let tmp_path = format!("{data_dir}/menu.json.tmp");

    // Construire le JSON final du menu
    let menu_with_metadata = serde_json::json!({
        "version": remote.version,
        "last_updated_ms": remote.updated_at_ms,
        "items": remote.menu.get("items").cloned().unwrap_or(serde_json::json!([])),
    });

    let json = match serde_json::to_string_pretty(&menu_with_metadata) {
        Ok(j) => j,
        Err(e) => {
            warn!(error = %e, "Impossible de sérialiser la configuration reçue");
            return;
        }
    };

    // Écriture dans le fichier temporaire
    if let Err(e) = std::fs::write(&tmp_path, &json) {
        warn!(error = %e, path = %tmp_path, "Impossible d'écrire le fichier menu temporaire");
        return;
    }

    // Renommage atomique
    if let Err(e) = std::fs::rename(&tmp_path, &menu_path) {
        warn!(error = %e, from = %tmp_path, to = %menu_path, "Impossible de renommer menu.json.tmp");
        // Tenter de nettoyer le fichier temporaire
        let _ = std::fs::remove_file(&tmp_path);
        return;
    }

    info!(
        path = %menu_path,
        version = remote.version,
        "menu.json mis à jour avec succès"
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn temp_dir() -> TempDir {
        tempfile::tempdir().expect("TempDir")
    }

    fn make_remote_config(version: u32) -> RemoteConfig {
        RemoteConfig {
            version,
            updated_at_ms: 1_700_000_000_000,
            menu: serde_json::json!({
                "items": [
                    {"id": "001", "name": "Burger", "price_ttc_cents": 1200}
                ]
            }),
            site_id: "SITE-001".to_string(),
        }
    }

    #[test]
    fn read_local_version_missing_file_returns_zero() {
        let dir = temp_dir();
        let version = read_local_version(dir.path().to_str().unwrap());
        assert_eq!(version, 0);
    }

    #[test]
    fn read_local_version_from_valid_file() {
        let dir = temp_dir();
        let data_dir = dir.path().to_str().unwrap();
        let json = serde_json::json!({"version": 42, "items": []}).to_string();
        fs::write(format!("{data_dir}/menu.json"), &json).unwrap();

        let version = read_local_version(data_dir);
        assert_eq!(version, 42);
    }

    #[test]
    fn read_local_version_invalid_json_returns_zero() {
        let dir = temp_dir();
        let data_dir = dir.path().to_str().unwrap();
        fs::write(format!("{data_dir}/menu.json"), "not json").unwrap();
        assert_eq!(read_local_version(data_dir), 0);
    }

    #[test]
    fn apply_config_writes_menu_json() {
        let dir = temp_dir();
        let data_dir = dir.path().to_str().unwrap();
        let remote = make_remote_config(5);

        apply_config(&remote, data_dir);

        let menu_path = format!("{data_dir}/menu.json");
        assert!(Path::new(&menu_path).exists(), "menu.json doit exister après apply_config");

        let content = fs::read_to_string(&menu_path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(value["version"], 5);
    }

    #[test]
    fn apply_config_no_tmp_file_left() {
        let dir = temp_dir();
        let data_dir = dir.path().to_str().unwrap();
        let remote = make_remote_config(3);

        apply_config(&remote, data_dir);

        let tmp_path = format!("{data_dir}/menu.json.tmp");
        assert!(
            !Path::new(&tmp_path).exists(),
            "Le fichier temporaire ne doit pas rester après apply_config"
        );
    }

    #[test]
    fn apply_config_is_idempotent() {
        let dir = temp_dir();
        let data_dir = dir.path().to_str().unwrap();
        let remote = make_remote_config(7);

        apply_config(&remote, data_dir);
        apply_config(&remote, data_dir); // deuxième appel

        let content = fs::read_to_string(format!("{data_dir}/menu.json")).unwrap();
        let value: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(value["version"], 7);
    }
}
