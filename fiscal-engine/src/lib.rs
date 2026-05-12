//! # fiscal-engine
//!
//! Moteur fiscal certifiable NF525 pour systèmes de caisse QSR.
//!
//! Ce crate est le **composant le plus critique** du système `pos-fiscal`.
//! Il est conçu pour être auditable par un laboratoire LNE/Infocert :
//! chaque contrainte réglementaire est explicitée dans le code et dans les tests.
//!
//! ## Architecture interne
//!
//! ```text
//! fiscal-engine/
//! ├── hash_engine/
//! │   └── mod.rs          — Hash SHA-256 chaîné, verify_chain_integrity()
//! ├── types/
//! │   ├── tva.rs          — Taux de TVA, décomposition par taux
//! │   ├── operation.rs    — Types d'opérations fiscales
//! │   ├── entry.rs        — FiscalEntry : enregistrement du journal
//! │   ├── session.rs      — Session de caisse (entre deux rapports Z)
//! │   ├── z_report.rs     — Rapport Z de clôture
//! │   └── archive.rs      — Export annuel signé
//! └── errors/
//!     └── mod.rs          — Hiérarchie d'erreurs (thiserror)
//! ```
//!
//! ## Contraintes NF525 implémentées
//!
//! - **Append-only** : aucune modification d'entrée existante (§4.1)
//! - **Séquence continue** : numéros séquentiels sans trou (§4.2)
//! - **Hash chaîné SHA-256** : chaque entrée contient le hash de la précédente (§5)
//! - **Rapport Z immuable** : généré une seule fois par session (§6)
//! - **Archivage annuel signé Ed25519** : format CSV normé + signature (§7)

#![deny(clippy::all, clippy::pedantic)]
#![warn(missing_docs)]

pub mod archive_engine;
pub mod errors;
pub mod hash_engine;
pub mod journal;
pub mod types;
pub mod z_report_engine;

// Re-exports de commodité — l'API publique stable du crate
pub use archive_engine::{
    export_archive, generate_signing_keypair, persist_archive_metadata, sign_archive,
    signing_key_from_bytes, verify_archive_signature,
};
pub use errors::{ArchiveError, FiscalError, HashError, IntegrityError, SessionError};
pub use hash_engine::{
    compute_entry_hash, hex_decode, hex_encode, verify_chain_integrity, verify_entry_hash,
    HashInput, IntegrityReport,
};
pub use journal::Journal;
pub use types::{
    archive::{ArchiveExport, SignedArchive},
    entry::FiscalEntry,
    operation::OperationType,
    session::{Session, SessionStatus},
    tva::{TvaBreakdown, TvaRate},
    z_report::ZReport,
};
pub use z_report_engine::{format_z_report_text, generate_z_report, load_z_report};
