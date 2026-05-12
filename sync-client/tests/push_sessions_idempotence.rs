// Test d'intégration : idempotence de push_sessions sur (site_id, session_ref).
//
// Scénario simulé : crash entre push_sessions réussi et mark_sessions_synced.
// Au cycle suivant, la session est encore synced=0 localement mais existe déjà
// dans Supabase. Le mock retourne [] (ignore-duplicates). Le cycle doit réussir
// et marquer la session comme synced=1 sans bloquer le push des entrées.

use sqlx::sqlite::SqlitePoolOptions;
use tempfile::NamedTempFile;
use wiremock::{Mock, MockServer, ResponseTemplate};
use wiremock::matchers::{method, path, path_regex};

use sync_client::{client::SupabaseClient, config::SyncConfig, sync_loop::run_sync_cycle};
use fiscal_engine::{
    journal::store::JournalStore,
    types::{operation::OperationType, session::Session, tva::TvaRate},
    hash_engine::build_entry_for_test,
};
use common::GENESIS_HASH;

async fn setup_store(db_path: &str) -> JournalStore {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&format!("sqlite:{}", db_path))
        .await
        .expect("open sqlite");
    let store = JournalStore::new(pool).await.expect("JournalStore::new");
    store.run_migrations().await.expect("migrations");
    store
}

fn make_config(db_path: &str, supabase_url: &str) -> SyncConfig {
    SyncConfig {
        database_url:         format!("sqlite:{}", db_path),
        supabase_url:         supabase_url.to_string(),
        supabase_service_key: "test-key".to_string(),
        site_id:              "SITE-001".to_string(),
        sync_interval_secs:   30,
        batch_size:           100,
        max_retries:          1,
        backoff_initial_ms:   10,
        backoff_max_secs:     1,
        http_timeout_secs:    10,
        data_dir:             "./test_data".to_string(),
    }
}

/// Cycle normal : session + entrées poussées pour la première fois.
#[tokio::test]
async fn first_sync_cycle_pushes_session_and_entries() {
    let db_file = NamedTempFile::new().unwrap();
    let db_path = db_file.path().to_str().unwrap();
    let store = setup_store(db_path).await;

    let session = Session::new(1, 1_700_000_000_000);
    let session_uuid = session.id.0;
    store.insert_session(&session).await.unwrap();
    let sid: [u8; 16] = session_uuid.as_bytes()[..16].try_into().unwrap();
    let e1 = build_entry_for_test(1, sid, OperationType::Sale, 1000, TvaRate::Normal20, 1_700_000_001_000, GENESIS_HASH);
    let mut tx = store.begin_transaction().await.unwrap();
    store.insert_entry(&e1, &mut tx).await.unwrap();
    tx.commit().await.unwrap();

    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/rest/v1/"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server).await;
    Mock::given(method("POST")).and(path("/rest/v1/sessions"))
        .respond_with(ResponseTemplate::new(201).set_body_json(
            serde_json::json!([{"id": session_uuid.to_string()}])
        ))
        .mount(&server).await;
    Mock::given(method("POST")).and(path("/rest/v1/fiscal_entries"))
        .respond_with(ResponseTemplate::new(201).set_body_json(
            serde_json::json!([{"id": e1.id.0.to_string()}])
        ))
        .mount(&server).await;
    Mock::given(method("GET")).and(path_regex("/rest/v1/site_configs.*"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server).await;

    let config = make_config(db_path, &server.uri());
    let client = SupabaseClient::new_for_test(&server.uri(), "test-key");
    let m = run_sync_cycle(&store, &client, &config).await.unwrap();

    assert!(!m.was_offline);
    assert_eq!(m.batches_failed, 0);
    assert_eq!(m.sessions_pushed, 1);
    assert_eq!(m.entries_pushed, 1);
    assert_eq!(store.load_unsynced_sessions().await.unwrap().len(), 0);
    assert_eq!(store.load_unsynced_entries(100).await.unwrap().len(), 0);
}

/// Scénario crash-recovery : la session existe déjà dans Supabase (Supabase retourne []),
/// mais est encore synced=0 localement. Le cycle doit réussir et marquer la session synced=1.
#[tokio::test]
async fn crash_recovery_session_already_in_supabase_still_marks_synced() {
    let db_file = NamedTempFile::new().unwrap();
    let db_path = db_file.path().to_str().unwrap();
    let store = setup_store(db_path).await;

    let session = Session::new(1, 1_700_000_000_000);
    let session_uuid = session.id.0;
    store.insert_session(&session).await.unwrap();
    let sid: [u8; 16] = session_uuid.as_bytes()[..16].try_into().unwrap();
    let e1 = build_entry_for_test(1, sid, OperationType::Sale, 1000, TvaRate::Normal20, 1_700_000_001_000, GENESIS_HASH);
    let mut tx = store.begin_transaction().await.unwrap();
    store.insert_entry(&e1, &mut tx).await.unwrap();
    tx.commit().await.unwrap();

    assert_eq!(store.load_unsynced_sessions().await.unwrap().len(), 1);
    assert_eq!(store.load_unsynced_entries(100).await.unwrap().len(), 1);

    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/rest/v1/"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server).await;
    // Supabase retourne [] : ignore-duplicates a ignoré la session (déjà présente)
    Mock::given(method("POST")).and(path("/rest/v1/sessions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server).await;
    Mock::given(method("POST")).and(path("/rest/v1/fiscal_entries"))
        .respond_with(ResponseTemplate::new(201).set_body_json(
            serde_json::json!([{"id": e1.id.0.to_string()}])
        ))
        .mount(&server).await;
    Mock::given(method("GET")).and(path_regex("/rest/v1/site_configs.*"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server).await;

    let config = make_config(db_path, &server.uri());
    let client = SupabaseClient::new_for_test(&server.uri(), "test-key");
    let m = run_sync_cycle(&store, &client, &config).await.unwrap();

    assert!(!m.was_offline);
    assert_eq!(m.batches_failed, 0, "aucun batch ne doit échouer");
    assert_eq!(m.sessions_pushed, 1, "session marquée synced localement");
    assert_eq!(m.entries_pushed, 1, "entrée poussée avec succès");
    assert_eq!(store.load_unsynced_sessions().await.unwrap().len(), 0);
    assert_eq!(store.load_unsynced_entries(100).await.unwrap().len(), 0);
}

/// Deuxième cycle sans rien à pousser : no-op complet.
#[tokio::test]
async fn second_cycle_noop_when_everything_synced() {
    let db_file = NamedTempFile::new().unwrap();
    let db_path = db_file.path().to_str().unwrap();
    let store = setup_store(db_path).await;

    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/rest/v1/"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server).await;
    Mock::given(method("GET")).and(path_regex("/rest/v1/site_configs.*"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server).await;

    let config = make_config(db_path, &server.uri());
    let client = SupabaseClient::new_for_test(&server.uri(), "test-key");
    let m = run_sync_cycle(&store, &client, &config).await.unwrap();

    assert_eq!(m.sessions_pushed, 0);
    assert_eq!(m.entries_pushed, 0);
    assert_eq!(m.batches_failed, 0);
}
