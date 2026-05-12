use std::env;
use sqlx::sqlite::SqlitePoolOptions;
use tempfile::NamedTempFile;
use sync_client::{client::SupabaseClient, config::SyncConfig, serializer::SessionPayload, sync_loop::run_sync_cycle};
use fiscal_engine::{
    journal::store::JournalStore,
    types::{operation::OperationType, session::Session, tva::TvaRate},
    hash_engine::build_entry_for_test,
};
use common::GENESIS_HASH;

fn make_config(db_path: &str) -> SyncConfig {
    SyncConfig {
        database_url:         format!("sqlite:{}", db_path),
        supabase_url:         env::var("SUPABASE_URL").expect("SUPABASE_URL requis"),
        supabase_service_key: env::var("SUPABASE_SERVICE_KEY").expect("SUPABASE_SERVICE_KEY requis"),
        site_id:              env::var("SITE_ID").expect("SITE_ID requis"),
        sync_interval_secs:   30,
        batch_size:           100,
        max_retries:          3,
        backoff_initial_ms:   100,
        backoff_max_secs:     5,
        http_timeout_secs:    30,
        data_dir:             "./test_data".to_string(),
    }
}

async fn setup_db(db_path: &str) -> JournalStore {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&format!("sqlite:{}", db_path))
        .await
        .expect("Impossible d'ouvrir SQLite");
    let store = JournalStore::new(pool).await.expect("JournalStore::new echoue");
    store.run_migrations().await.expect("Migrations echouees");
    store
}

async fn seed_local_db(store: &JournalStore) -> (uuid::Uuid, Vec<uuid::Uuid>) {
    let session = Session::new(1, 1_700_000_000_000);
    let session_uuid = session.id.0;
    store.insert_session(&session).await.expect("insert_session echoue");

    let sid: [u8; 16] = session_uuid.as_bytes()[..16].try_into().unwrap();
    let e1 = build_entry_for_test(1, sid, OperationType::Sale,   1200, TvaRate::Normal20,        1_700_000_001_000, GENESIS_HASH);
    let e2 = build_entry_for_test(2, sid, OperationType::Sale,    850, TvaRate::Intermediaire10,  1_700_000_002_000, e1.hash);
    let e3 = build_entry_for_test(3, sid, OperationType::Sale,   2000, TvaRate::Reduit5_5,         1_700_000_003_000, e2.hash);

    let uuids = vec![e1.id.0, e2.id.0, e3.id.0];
    for entry in [&e1, &e2, &e3] {
        let mut tx = store.begin_transaction().await.expect("begin_transaction echoue");
        store.insert_entry(entry, &mut tx).await.expect("insert_entry echoue");
        tx.commit().await.expect("commit echoue");
    }
    (session_uuid, uuids)
}

async fn verify_session_in_supabase(config: &SyncConfig, session_uuid: &str) -> bool {
    let client = reqwest::Client::new();
    let url = format!("{}/rest/v1/sessions?id=eq.{}&select=id",
        config.supabase_url.trim_end_matches('/'), session_uuid);
    let resp = client.get(&url)
        .header("apikey", &config.supabase_service_key)
        .header("Authorization", format!("Bearer {}", config.supabase_service_key))
        .send().await.expect("GET sessions echoue");
    let rows: serde_json::Value = resp.json().await.expect("JSON invalide");
    rows.as_array().map(|a| !a.is_empty()).unwrap_or(false)
}

async fn count_entries_in_supabase(config: &SyncConfig, session_uuid: &str) -> usize {
    let client = reqwest::Client::new();
    let url = format!("{}/rest/v1/fiscal_entries?session_id=eq.{}&select=id",
        config.supabase_url.trim_end_matches('/'), session_uuid);
    let resp = client.get(&url)
        .header("apikey", &config.supabase_service_key)
        .header("Authorization", format!("Bearer {}", config.supabase_service_key))
        .send().await.expect("GET fiscal_entries echoue");
    let rows: serde_json::Value = resp.json().await.expect("JSON invalide");
    rows.as_array().map(|a| a.len()).unwrap_or(0)
}

async fn cleanup_supabase(config: &SyncConfig, _session_uuid: &str) {
    let client = reqwest::Client::new();
    let url = format!("{}/rest/v1/rpc/delete_test_data",
        config.supabase_url.trim_end_matches('/'));
    let body = serde_json::json!({ "p_site_id": config.site_id });
    match client
        .post(&url)
        .header("apikey", &config.supabase_service_key)
        .header("Authorization", format!("Bearer {}", config.supabase_service_key))
        .json(&body)
        .send().await
    {
        Ok(r) if r.status().is_success() =>
            println!("[cleanup] delete_test_data() OK"),
        Ok(r) =>
            println!("[cleanup] AVERTISSEMENT status={}", r.status()),
        Err(e) =>
            println!("[cleanup] ERREUR : {}", e),
    }
}

#[tokio::test]
#[ignore = "Necessite SUPABASE_SERVICE_KEY et reseau actif"]
async fn test_e2e_sync_sqlite_to_supabase() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("sync_client=debug,fiscal_engine=info")
        .try_init();

    println!("\n=== TEST E2E : SQLite -> Supabase ===\n");

    let db_file = NamedTempFile::new().expect("NamedTempFile echoue");
    let db_path = db_file.path().to_str().unwrap().to_string();
    let config = make_config(&db_path);

    println!("[setup] SQLite   : {}", db_path);
    println!("[setup] Site ID  : {}", config.site_id);
    println!("[setup] Supabase : {}", config.supabase_url);

    let store = setup_db(&db_path).await;
    println!("[setup] Migrations OK");

    let (session_uuid, entry_uuids) = seed_local_db(&store).await;
    let suuid = session_uuid.to_string();
    println!("[seed] Session : {}", suuid);
    for (i, uid) in entry_uuids.iter().enumerate() {
        println!("[seed] Entree #{} : {}", i + 1, uid);
    }

    assert_eq!(store.load_unsynced_sessions().await.unwrap().len(), 1);
    assert_eq!(store.load_unsynced_entries(100).await.unwrap().len(), 3);
    println!("[pre-sync] 1 session + 3 entrees non-sync OK");

    let client = SupabaseClient::new(&config).expect("SupabaseClient::new echoue");
    assert!(client.is_reachable().await, "Supabase inaccessible");
    println!("[connectivity] Supabase accessible OK");

    println!("[sync] run_sync_cycle...");
    let m = run_sync_cycle(&store, &client, &config).await.expect("run_sync_cycle echoue");
    println!("[sync] {}ms | sessions={} entrees={} echecs={}", m.duration_ms, m.sessions_pushed, m.entries_pushed, m.batches_failed);

    assert!(!m.was_offline,         "Ne doit pas etre offline");
    assert_eq!(m.batches_failed,  0, "Aucun batch echoue");
    assert_eq!(m.sessions_pushed, 1, "1 session poussee");
    assert_eq!(m.entries_pushed,  3, "3 entrees poussees");

    assert_eq!(store.load_unsynced_sessions().await.unwrap().len(), 0, "Sessions synced=1");
    assert_eq!(store.load_unsynced_entries(100).await.unwrap().len(), 0, "Entrees synced=1");
    println!("[post-sync] SQLite synced=1 OK");

    assert!(verify_session_in_supabase(&config, &suuid).await, "Session absente de Supabase");
    println!("[verify] Session dans Supabase OK");

    let count = count_entries_in_supabase(&config, &suuid).await;
    assert_eq!(count, 3, "3 entrees attendues dans Supabase");
    println!("[verify] 3 entrees dans Supabase OK");

    println!("[idempotence] 2e cycle...");
    let m2 = run_sync_cycle(&store, &client, &config).await.expect("2e cycle echoue");
    assert_eq!(m2.sessions_pushed, 0);
    assert_eq!(m2.entries_pushed,  0);
    assert_eq!(m2.batches_failed,  0);
    println!("[idempotence] 2e cycle no-op OK");

    println!("[cleanup] Suppression donnees de test...");
    cleanup_supabase(&config, &suuid).await;

    println!("\n=== TEST E2E REUSSI ===\n");
}

/// Simule un crash entre push_sessions réussi et mark_sessions_synced.
/// La session est déjà dans Supabase mais encore synced=0 localement.
/// Le cycle suivant doit réussir : ignore-duplicates retourne [], la session
/// est marquée synced et les entrées sont poussées sans erreur FK.
#[tokio::test]
#[ignore = "Necessite SUPABASE_SERVICE_KEY et reseau actif"]
async fn test_e2e_idempotence_crash_recovery() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("sync_client=debug,fiscal_engine=info")
        .try_init();

    println!("\n=== TEST E2E IDEMPOTENCE CRASH-RECOVERY ===\n");

    let db_file = NamedTempFile::new().expect("NamedTempFile echoue");
    let db_path = db_file.path().to_str().unwrap().to_string();
    let config = make_config(&db_path);
    let store = setup_db(&db_path).await;
    let client = SupabaseClient::new(&config).expect("SupabaseClient::new echoue");

    assert!(client.is_reachable().await, "Supabase inaccessible");

    let (session_uuid, _entry_uuids) = seed_local_db(&store).await;
    let suuid = session_uuid.to_string();
    println!("[seed] Session : {}", suuid);

    // Étape 1 : pousser la session vers Supabase sans appeler mark_sessions_synced
    // (simule un crash entre les deux opérations)
    let sessions = store.load_unsynced_sessions().await.unwrap();
    let payloads: Vec<SessionPayload> = sessions.iter()
        .map(|s| SessionPayload::from_session(s, &config.site_id))
        .collect();
    let inserted = client.push_sessions(&payloads).await.expect("1er push_sessions echoue");
    assert_eq!(inserted, 1, "1 session inseree au 1er push");
    println!("[crash-sim] Session poussee dans Supabase, mark_sessions_synced NON appele");

    // La session est encore synced=0 localement
    assert_eq!(store.load_unsynced_sessions().await.unwrap().len(), 1, "session encore non-sync");
    assert!(verify_session_in_supabase(&config, &suuid).await, "Session absente de Supabase apres 1er push");
    println!("[crash-sim] synced=0 local + presente dans Supabase : crash bien simule");

    // Étape 2 : run_sync_cycle — la session est un doublon, ignore-duplicates retourne []
    // Le cycle doit marquer la session synced ET pousser les entrées
    println!("[recovery] run_sync_cycle...");
    let m = run_sync_cycle(&store, &client, &config).await.expect("run_sync_cycle echoue");
    println!("[recovery] {}ms | sessions={} entrees={} echecs={}", m.duration_ms, m.sessions_pushed, m.entries_pushed, m.batches_failed);

    assert_eq!(m.batches_failed,  0, "aucun batch echoue");
    assert_eq!(m.sessions_pushed, 1, "session marquee synced localement");
    assert_eq!(m.entries_pushed,  3, "3 entrees poussees");

    assert_eq!(store.load_unsynced_sessions().await.unwrap().len(), 0, "session synced=1 apres recovery");
    assert_eq!(store.load_unsynced_entries(100).await.unwrap().len(), 0, "entrees synced=1 apres recovery");

    let count = count_entries_in_supabase(&config, &suuid).await;
    assert_eq!(count, 3, "3 entrees dans Supabase");
    println!("[verify] 3 entrees dans Supabase OK");

    println!("[cleanup] Suppression donnees de test...");
    cleanup_supabase(&config, &suuid).await;

    println!("\n=== TEST E2E IDEMPOTENCE CRASH-RECOVERY REUSSI ===\n");
}
