//! # types
//!
//! Types métier du moteur fiscal.
//! Chaque sous-module correspond à un concept fiscal distinct.
//!
//! ## Règle d'immuabilité
//! Tous les types de ce module sont `pub` mais leurs champs internes
//! sont accessibles en lecture uniquement depuis l'extérieur du crate.
//! La mutation n'est possible que via les fonctions du `fiscal-engine`.

pub mod archive;
pub mod entry;
pub mod operation;
pub mod session;
pub mod tva;
pub mod z_report;
