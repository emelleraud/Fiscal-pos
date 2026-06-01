//! # errors
//!
//! Hiérarchie d'erreurs du `fiscal-engine`, définie avec `thiserror`.
//!
//! ## Philosophie
//! Chaque domaine (hash, intégrité, session, archive) a son propre type d'erreur.
//! `FiscalError` est l'erreur racine qui les agrège pour les consommateurs externes
//! (typiquement `edge-api`).
//!
//! Les erreurs sont conçues pour être **loggables** (implémentent `Display` via thiserror)
//! et **sérialisables en JSON** pour la réponse API (via `ApiError` du crate `common`).
//!
//! ## Règle d'or
//! Aucune erreur de ce module ne contient de `String` brute comme seul champ :
//! chaque variante porte un contexte structuré permettant un diagnostic précis en audit.

use thiserror::Error;

// ---------------------------------------------------------------------------
// Erreur racine
// ---------------------------------------------------------------------------

/// Erreur racine du moteur fiscal.
///
/// Agrège toutes les sous-erreurs des domaines internes.
/// C'est le type retourné par toutes les fonctions publiques du crate.
///
/// # Examples
/// ```
/// use fiscal_engine::errors::FiscalError;
/// let err = FiscalError::SessionClosed { session_id: "abc".to_string() };
/// assert!(err.to_string().contains("abc"));
/// ```
#[derive(Debug, Error)]
pub enum FiscalError {
    /// Erreur dans le calcul ou la vérification du hash chaîné.
    #[error("Erreur de hash fiscal : {0}")]
    Hash(#[from] HashError),

    /// Violation de l'intégrité de la chaîne fiscale.
    #[error("Violation d'intégrité fiscale : {0}")]
    Integrity(#[from] IntegrityError),

    /// Erreur liée à l'état d'une session de caisse.
    #[error("Erreur de session : {0}")]
    Session(#[from] SessionError),

    /// Erreur lors de la génération ou signature d'une archive.
    #[error("Erreur d'archivage : {0}")]
    Archive(#[from] ArchiveError),

    /// La session est fermée : aucune nouvelle opération n'est acceptée.
    ///
    /// NF525 §4.1 : les opérations sur une session clôturée sont interdites.
    #[error("La session '{session_id}' est clôturée, aucune opération n'est possible")]
    SessionClosed {
        /// Identifiant de la session concernée.
        session_id: String,
    },

    /// Montant invalide : les montants doivent être positifs pour une vente,
    /// négatifs pour un remboursement.
    #[error("Montant invalide : {amount_cents} centimes pour l'opération '{operation}'")]
    InvalidAmount {
        /// Montant en centimes reçu.
        amount_cents: i64,
        /// Nom de l'opération concernée.
        operation: String,
    },

    /// Taux de TVA incohérent avec le montant déclaré.
    #[error(
        "Décomposition TVA invalide : total HT {ht_cents} + TVA {tva_cents} ≠ TTC {ttc_cents}"
    )]
    InvalidTvaDecomposition {
        /// Montant hors taxe en centimes.
        ht_cents: i64,
        /// Montant de TVA en centimes.
        tva_cents: i64,
        /// Montant TTC en centimes.
        ttc_cents: i64,
    },

    /// Erreur de persistence (`SQLite`). Encapsule `sqlx::Error`.
    #[error("Erreur de persistence : {0}")]
    Database(#[from] sqlx::Error),
}

// ---------------------------------------------------------------------------
// Erreurs de hash
// ---------------------------------------------------------------------------

/// Erreurs relatives au calcul SHA-256 de la chaîne fiscale.
#[derive(Debug, Error)]
pub enum HashError {
    /// La taille du hash reçu n'est pas de 32 octets.
    #[error("Taille de hash invalide : attendu 32 octets, reçu {received} octets")]
    InvalidHashSize {
        /// Nombre d'octets reçus.
        received: usize,
    },

    /// Le hash précédent référencé est introuvable dans le journal.
    #[error("Hash précédent introuvable pour la séquence {sequence}")]
    PreviousHashNotFound {
        /// Numéro de séquence de l'entrée orpheline.
        sequence: u64,
    },

    /// Erreur de sérialisation des données à hasher.
    #[error("Échec de sérialisation pour le hash de la séquence {sequence} : {source}")]
    SerializationFailed {
        /// Numéro de séquence concerné.
        sequence: u64,
        /// Erreur de sérialisation source.
        source: serde_json::Error,
    },
}

// ---------------------------------------------------------------------------
// Erreurs d'intégrité
// ---------------------------------------------------------------------------

/// Violations de l'intégrité de la chaîne fiscale NF525.
///
/// Ces erreurs indiquent une **tentative de falsification** ou une corruption du journal.
/// Toute occurrence doit être immédiatement loggée et bloquante.
#[derive(Debug, Error)]
pub enum IntegrityError {
    /// Le hash calculé ne correspond pas au hash stocké.
    ///
    /// NF525 §5.2 : toute discordance de hash est une violation critique.
    #[error(
        "Hash invalide à la séquence {sequence} : \
         attendu {expected_hex}, calculé {computed_hex}"
    )]
    HashMismatch {
        /// Numéro de séquence de l'entrée corrompue.
        sequence: u64,
        /// Hash attendu (tel que stocké dans le journal), encodé en hexadécimal.
        expected_hex: String,
        /// Hash recalculé à partir des données, encodé en hexadécimal.
        computed_hex: String,
    },

    /// Un numéro de séquence est manquant dans la chaîne.
    ///
    /// NF525 §4.2 : la séquence doit être continue, sans trou.
    #[error(
        "Trou dans la séquence fiscale : attendu {expected_sequence}, trouvé {found_sequence}"
    )]
    SequenceGap {
        /// Numéro de séquence attendu.
        expected_sequence: u64,
        /// Numéro de séquence trouvé à la place.
        found_sequence: u64,
    },

    /// Le premier enregistrement ne référence pas le hash genesis (`[0u8; 32]`).
    ///
    /// NF525 §5.1 : le vecteur d'initialisation doit être nul.
    #[error(
        "Le premier enregistrement (séquence {sequence}) \
         ne référence pas le hash genesis (vecteur nul)"
    )]
    InvalidGenesisHash {
        /// Numéro de séquence du premier enregistrement.
        sequence: u64,
    },

    /// Le journal est vide alors qu'une opération l'exige non-vide.
    #[error("Le journal fiscal est vide")]
    EmptyJournal,

    /// La chaîne est corrompue à partir d'une séquence donnée
    /// (premier point de défaillance détecté).
    #[error(
        "Chaîne fiscale corrompue à partir de la séquence {first_failed_sequence} \
         ({total_failures} violation(s) détectée(s) sur {total_checked} entrées vérifiées)"
    )]
    ChainCorrupted {
        /// Première séquence où une violation a été détectée.
        first_failed_sequence: u64,
        /// Nombre total de violations trouvées.
        total_failures: usize,
        /// Nombre total d'entrées vérifiées.
        total_checked: usize,
    },
}

// ---------------------------------------------------------------------------
// Erreurs de session
// ---------------------------------------------------------------------------

/// Erreurs relatives à la gestion des sessions de caisse.
#[derive(Debug, Error)]
pub enum SessionError {
    /// Aucune session active n'existe. Une session doit être ouverte avant toute opération.
    #[error("Aucune session active : ouvrir une session avant d'enregistrer des opérations")]
    NoActiveSession,

    /// Une session est déjà active. Une seule session peut être ouverte à la fois.
    #[error("Une session '{session_id}' est déjà active sur ce terminal")]
    SessionAlreadyActive {
        /// Identifiant de la session active existante.
        session_id: String,
    },

    /// Tentative de clôture d'une session déjà clôturée.
    #[error("La session '{session_id}' a déjà été clôturée le {closed_at}")]
    SessionAlreadyClosed {
        /// Identifiant de la session.
        session_id: String,
        /// Date de clôture précédente (ISO 8601).
        closed_at: String,
    },

    /// Le rapport Z a déjà été généré pour cette session.
    ///
    /// NF525 §6 : un rapport Z ne peut être généré qu'une seule fois par session.
    #[error("Le rapport Z existe déjà pour la session '{session_id}'")]
    ZReportAlreadyGenerated {
        /// Identifiant de la session.
        session_id: String,
    },
}

// ---------------------------------------------------------------------------
// Erreurs d'archivage
// ---------------------------------------------------------------------------

/// Erreurs relatives à la génération et signature des archives annuelles.
#[derive(Debug, Error)]
pub enum ArchiveError {
    /// L'année demandée ne contient aucune entrée fiscale.
    #[error("Aucune entrée fiscale pour l'année {year}")]
    NoDataForYear {
        /// Année fiscale demandée.
        year: u32,
    },

    /// L'archive a déjà été générée pour cette année.
    ///
    /// NF525 §7 : une seule archive par année civile.
    #[error("L'archive de l'année {year} a déjà été générée")]
    ArchiveAlreadyExists {
        /// Année fiscale.
        year: u32,
    },

    /// Erreur lors de la génération du fichier CSV.
    #[error("Erreur de génération CSV pour l'année {year} : {reason}")]
    CsvGenerationFailed {
        /// Année fiscale.
        year: u32,
        /// Description de l'erreur.
        reason: String,
    },

    /// Erreur lors de la signature Ed25519 de l'archive.
    #[error("Échec de signature Ed25519 pour l'archive {year} : {reason}")]
    SigningFailed {
        /// Année fiscale.
        year: u32,
        /// Description de l'erreur de signature.
        reason: String,
    },

    /// Clé de signature absente ou invalide.
    #[error("Clé de signature Ed25519 invalide ou absente : {reason}")]
    InvalidSigningKey {
        /// Description du problème de clé.
        reason: String,
    },
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fiscal_error_from_hash_error() {
        let hash_err = HashError::InvalidHashSize { received: 16 };
        let fiscal_err = FiscalError::from(hash_err);
        let msg = fiscal_err.to_string();
        assert!(
            msg.contains("16"),
            "Le message doit contenir la taille reçue"
        );
        assert!(msg.contains("hash"), "Le message doit mentionner 'hash'");
    }

    #[test]
    fn fiscal_error_from_integrity_error() {
        let int_err = IntegrityError::SequenceGap {
            expected_sequence: 5,
            found_sequence: 7,
        };
        let fiscal_err = FiscalError::from(int_err);
        let msg = fiscal_err.to_string();
        assert!(
            msg.contains("5"),
            "Le message doit contenir la séquence attendue"
        );
        assert!(
            msg.contains("7"),
            "Le message doit contenir la séquence trouvée"
        );
    }

    #[test]
    fn integrity_error_hash_mismatch_display() {
        let err = IntegrityError::HashMismatch {
            sequence: 42,
            expected_hex: "aabbcc".to_string(),
            computed_hex: "ddeeff".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("42"));
        assert!(msg.contains("aabbcc"));
        assert!(msg.contains("ddeeff"));
    }

    #[test]
    fn integrity_error_chain_corrupted_display() {
        let err = IntegrityError::ChainCorrupted {
            first_failed_sequence: 10,
            total_failures: 3,
            total_checked: 100,
        };
        let msg = err.to_string();
        assert!(msg.contains("10"));
        assert!(msg.contains('3'));
        assert!(msg.contains("100"));
    }

    #[test]
    fn session_error_already_active_display() {
        let err = SessionError::SessionAlreadyActive {
            session_id: "sess-001".to_string(),
        };
        assert!(err.to_string().contains("sess-001"));
    }

    #[test]
    fn archive_error_no_data_display() {
        let err = ArchiveError::NoDataForYear { year: 2024 };
        assert!(err.to_string().contains("2024"));
    }

    #[test]
    fn fiscal_error_invalid_amount_display() {
        let err = FiscalError::InvalidAmount {
            amount_cents: -500,
            operation: "Sale".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("-500"));
        assert!(msg.contains("Sale"));
    }

    #[test]
    fn fiscal_error_invalid_tva_display() {
        let err = FiscalError::InvalidTvaDecomposition {
            ht_cents: 1000,
            tva_cents: 55,
            ttc_cents: 1100, // intentionnellement faux pour tester
        };
        let msg = err.to_string();
        assert!(msg.contains("1000"));
        assert!(msg.contains("55"));
        assert!(msg.contains("1100"));
    }
}
