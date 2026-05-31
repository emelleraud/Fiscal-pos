//! Crate sync-client — exposé comme lib pour les tests d'intégration.
//!
//! Les modules sont publics pour permettre à `tests/integration/` de les utiliser.
//! Le binaire `main.rs` importe depuis cette lib via `use sync_client::...`.

pub mod client;
pub mod config;
pub mod config_puller;
pub mod error;
pub mod promo_puller;
pub mod serializer;
pub mod sync_loop;
