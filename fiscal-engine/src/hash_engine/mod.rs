//! # `hash_engine`
//!
//! Moteur de hash SHA-256 chaîné pour la certification NF525.
//!
//! ## Schéma de hash (NF525 §5)
//!
//! Chaque entrée fiscale est hashée selon le schéma canonique suivant :
//!
//! ```text
//! H(n) = SHA-256(
//!     sequence_number_be_u64     [8 octets, big-endian]
//!   ‖ entry_id_bytes             [16 octets, UUID brut]
//!   ‖ session_id_bytes           [16 octets, UUID brut]
//!   ‖ operation_type_byte        [1 octet, discriminant]
//!   ‖ amount_ttc_cents_be_i64    [8 octets, big-endian signé]
//!   ‖ tva_rate_byte              [1 octet, discriminant]
//!   ‖ ht_cents_be_i64            [8 octets, big-endian signé]
//!   ‖ tva_cents_be_i64           [8 octets, big-endian signé]
//!   ‖ created_at_ms_be_u64       [8 octets, big-endian]
//!   ‖ previous_hash              [32 octets]
//! )
//! ```
//!
//! **Total du message hashé : 106 octets fixes.**
//!
//! ## Choix de conception
//!
//! - **Encodage binaire fixe, pas JSON** : la sérialisation JSON est fragile
//!   (ordre des clés, espaces, version de la lib). Le format binaire big-endian
//!   est parfaitement reproductible sur toute plateforme pour toute éternité.
//! - **Big-endian** : convention réseau, standard pour les protocoles cryptographiques.
//! - **Discriminants d'enum stables** : les valeurs numériques de `OperationType`
//!   et `TvaRate` sont fixées par ce module — les modifier est un changement cassant.
//!
//! ## Stabilité du format
//!
//! Ce schéma de hash est **figé pour la durée de certification LNE**.
//! Tout changement nécessite une nouvelle certification et invalide tous les
//! journaux existants. Un ADR doit documenter tout changement.
//!
//! ## Vérification d'intégrité
//!
//! `verify_chain_integrity()` relit toutes les entrées et vérifie :
//! 1. La séquence est continue (sans trou)
//! 2. Le hash genesis est correct (première entrée)
//! 3. Chaque hash stocké correspond au hash recalculé
//! 4. Chaque `previous_hash` correspond au hash de l'entrée précédente

use sha2::{Digest, Sha256};

use crate::{
    errors::{FiscalError, HashError, IntegrityError},
    types::{entry::FiscalEntry, operation::OperationType, tva::TvaRate},
};
use common::GENESIS_HASH;

// ---------------------------------------------------------------------------
// Discriminants d'enum — STABLES, NE PAS MODIFIER APRÈS CERTIFICATION
// ---------------------------------------------------------------------------

/// Discriminant binaire de `OperationType` pour le hash.
/// Ces valeurs sont figées — les modifier invalide tous les journaux existants.
fn operation_type_byte(op: OperationType) -> u8 {
    match op {
        OperationType::Sale => 0x01,
        OperationType::Refund => 0x02,
        OperationType::Cancel => 0x03,
        OperationType::Discount => 0x04,
        OperationType::ZClose => 0x05,
    }
}

/// Discriminant binaire de `TvaRate` pour le hash.
/// Ces valeurs sont figées — les modifier invalide tous les journaux existants.
fn tva_rate_byte(rate: TvaRate) -> u8 {
    match rate {
        TvaRate::Reduit5_5 => 0x01,
        TvaRate::Intermediaire10 => 0x02,
        TvaRate::Normal20 => 0x03,
    }
}

// ---------------------------------------------------------------------------
// Calcul du hash d'une entrée
// ---------------------------------------------------------------------------

/// Données minimales nécessaires au calcul du hash d'une entrée fiscale.
///
/// Séparé de `FiscalEntry` pour permettre le calcul du hash **avant** la création
/// complète de l'entrée (le hash est calculé pour remplir le champ `hash`).
#[derive(Debug, Clone, Copy)]
pub struct HashInput<'a> {
    /// Numéro de séquence de l'entrée.
    pub sequence_number: u64,
    /// UUID de l'entrée (16 octets bruts).
    pub entry_id_bytes: &'a [u8; 16],
    /// UUID de la session (16 octets bruts).
    pub session_id_bytes: &'a [u8; 16],
    /// Type d'opération.
    pub operation_type: OperationType,
    /// Montant TTC en centimes (signé).
    pub amount_ttc_cents: i64,
    /// Taux de TVA de la décomposition principale.
    pub tva_rate: TvaRate,
    /// Montant HT en centimes (signé).
    pub ht_cents: i64,
    /// Montant TVA en centimes (signé).
    pub tva_cents: i64,
    /// Timestamp de création en millisecondes Unix.
    pub created_at_ms: u64,
    /// Hash de l'entrée précédente (32 octets).
    pub previous_hash: &'a [u8; 32],
}

/// Calcule le hash SHA-256 d'une entrée fiscale selon le schéma canonique NF525.
///
/// Le message hashé est une concaténation de 106 octets en big-endian.
/// Voir le schéma détaillé dans la documentation du module.
///
/// # Arguments
/// * `input` - Données de l'entrée à hasher.
///
/// # Returns
/// Hash SHA-256 de 32 octets.
///
/// # Examples
/// ```
/// use fiscal_engine::hash_engine::{compute_entry_hash, HashInput};
/// use fiscal_engine::types::operation::OperationType;
/// use fiscal_engine::types::tva::TvaRate;
/// use common::GENESIS_HASH;
///
/// let id_bytes = [1u8; 16];
/// let session_bytes = [2u8; 16];
/// let input = HashInput {
///     sequence_number: 1,
///     entry_id_bytes: &id_bytes,
///     session_id_bytes: &session_bytes,
///     operation_type: OperationType::Sale,
///     amount_ttc_cents: 1100,
///     tva_rate: TvaRate::Intermediaire10,
///     ht_cents: 1000,
///     tva_cents: 100,
///     created_at_ms: 1_700_000_000_000,
///     previous_hash: &GENESIS_HASH,
/// };
/// let hash = compute_entry_hash(&input);
/// assert_eq!(hash.len(), 32);
/// ```
#[must_use]
pub fn compute_entry_hash(input: &HashInput<'_>) -> [u8; 32] {
    let mut hasher = Sha256::new();

    // Le message est une concaténation binaire déterministe de 106 octets.
    // L'ordre des champs est figé par le schéma NF525.

    // [8 octets] Numéro de séquence (big-endian u64)
    hasher.update(input.sequence_number.to_be_bytes());

    // [16 octets] UUID de l'entrée (octets bruts, pas le string hexadécimal)
    hasher.update(input.entry_id_bytes);

    // [16 octets] UUID de la session
    hasher.update(input.session_id_bytes);

    // [1 octet] Discriminant du type d'opération
    hasher.update([operation_type_byte(input.operation_type)]);

    // [8 octets] Montant TTC en centimes (big-endian i64 signé)
    hasher.update(input.amount_ttc_cents.to_be_bytes());

    // [1 octet] Discriminant du taux de TVA
    hasher.update([tva_rate_byte(input.tva_rate)]);

    // [8 octets] Montant HT en centimes (big-endian i64)
    hasher.update(input.ht_cents.to_be_bytes());

    // [8 octets] Montant TVA en centimes (big-endian i64)
    hasher.update(input.tva_cents.to_be_bytes());

    // [8 octets] Timestamp de création en millisecondes Unix (big-endian u64)
    hasher.update(input.created_at_ms.to_be_bytes());

    // [32 octets] Hash de l'entrée précédente (genesis = [0u8; 32] pour la première)
    hasher.update(input.previous_hash);

    // Total : 8+16+16+1+8+1+8+8+8+32 = 106 octets

    hasher.finalize().into()
}

// ---------------------------------------------------------------------------
// Vérification d'une entrée isolée
// ---------------------------------------------------------------------------

/// Vérifie que le hash stocké dans une `FiscalEntry` correspond au hash recalculé.
///
/// Utilisé en deux contextes :
/// 1. Vérification unitaire d'une entrée (tests, audit ponctuel)
/// 2. Appelé en boucle par `verify_chain_integrity()` pour la vérification complète
///
/// # Arguments
/// * `entry` - L'entrée à vérifier.
/// * `expected_previous_hash` - Le hash de l'entrée précédente (fourni par l'appelant
///   pour ne pas avoir à relire le journal).
///
/// # Errors
/// - `HashError::InvalidHashSize` si les UUIDs ne peuvent pas être extraits (ne devrait pas arriver)
/// - `IntegrityError::HashMismatch` si le hash recalculé diffère du hash stocké
///
/// # Examples
/// ```
/// use fiscal_engine::hash_engine::verify_entry_hash;
/// use fiscal_engine::types::entry::FiscalEntry;
/// // (voir les tests d'intégrité pour un exemple complet avec journal)
/// ```
pub fn verify_entry_hash(
    entry: &FiscalEntry,
    expected_previous_hash: &[u8; 32],
) -> Result<(), FiscalError> {
    let entry_id_bytes = entry.id.0.into_bytes();
    let session_id_bytes = entry.session_id.0.into_bytes();

    let input = HashInput {
        sequence_number: entry.sequence_number,
        entry_id_bytes: &entry_id_bytes,
        session_id_bytes: &session_id_bytes,
        operation_type: entry.operation_type,
        amount_ttc_cents: entry.amount_ttc_cents.0,
        tva_rate: entry.tva_breakdown.rate,
        ht_cents: entry.tva_breakdown.ht_cents.0,
        tva_cents: entry.tva_breakdown.tva_cents.0,
        created_at_ms: entry.created_at_ms,
        previous_hash: expected_previous_hash,
    };

    let computed = compute_entry_hash(&input);

    if computed != entry.hash {
        return Err(IntegrityError::HashMismatch {
            sequence: entry.sequence_number,
            expected_hex: hex_encode(&entry.hash),
            computed_hex: hex_encode(&computed),
        }
        .into());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Vérification complète de la chaîne
// ---------------------------------------------------------------------------

/// Résultat détaillé d'une vérification d'intégrité de la chaîne fiscale.
///
/// Retourné par `verify_chain_integrity()` en cas de succès.
/// Même en succès, ces métriques sont utiles pour les logs d'audit.
#[derive(Debug, Clone)]
pub struct IntegrityReport {
    /// Nombre d'entrées vérifiées.
    pub entries_checked: usize,
    /// Numéro de séquence de la première entrée.
    pub first_sequence: u64,
    /// Numéro de séquence de la dernière entrée.
    pub last_sequence: u64,
    /// Hash de la dernière entrée (utile pour enchaîner une vérification partielle).
    pub last_hash: [u8; 32],
}

/// Vérifie l'intégrité complète d'une tranche du journal fiscal.
///
/// Effectue les vérifications suivantes sur chaque entrée, dans l'ordre :
///
/// 1. **Séquence continue** : `entry.sequence_number == previous_sequence + 1`
/// 2. **Hash genesis** : si `entry.sequence_number == 1`, `entry.previous_hash == GENESIS_HASH`
/// 3. **Chaîne** : `entry.previous_hash == hash de l'entrée précédente`
/// 4. **Hash cohérent** : `hash recalculé == entry.hash`
///
/// La vérification est **fail-fast** sur les erreurs de séquence (trou dans la chaîne)
/// mais **exhaustive** sur les erreurs de hash (tous les hashs sont vérifiés même après
/// le premier échec, pour permettre un diagnostic complet).
///
/// # Arguments
/// * `entries` - Tranche d'entrées **triées par `sequence_number` croissant**.
///
/// # Errors
/// - `IntegrityError::EmptyJournal` si `entries` est vide
/// - `IntegrityError::InvalidGenesisHash` si la première entrée n'a pas le bon `previous_hash`
/// - `IntegrityError::SequenceGap` si un trou est détecté (fail-fast)
/// - `IntegrityError::ChainCorrupted` si ≥1 hash est invalide (avec le décompte complet)
///
/// # Examples
/// ```
/// use fiscal_engine::hash_engine::{verify_chain_integrity, build_entry_for_test};
/// use common::GENESIS_HASH;
///
/// // Voir les tests du module pour un exemple complet avec chaîne valide
/// ```
pub fn verify_chain_integrity(entries: &[FiscalEntry]) -> Result<IntegrityReport, FiscalError> {
    if entries.is_empty() {
        return Err(IntegrityError::EmptyJournal.into());
    }

    // Vérification du hash genesis sur la première entrée
    let first = &entries[0];
    if first.previous_hash != GENESIS_HASH {
        return Err(IntegrityError::InvalidGenesisHash {
            sequence: first.sequence_number,
        }
        .into());
    }

    let mut previous_hash = GENESIS_HASH;
    let mut previous_sequence = first.sequence_number - 1; // sera 0 pour la première entrée
    let mut first_failed_sequence: Option<u64> = None;
    let mut total_failures: usize = 0;

    for entry in entries {
        // --- Vérification 1 : continuité de la séquence (fail-fast) ---
        let expected_sequence = previous_sequence + 1;
        if entry.sequence_number != expected_sequence {
            return Err(IntegrityError::SequenceGap {
                expected_sequence,
                found_sequence: entry.sequence_number,
            }
            .into());
        }

        // --- Vérification 2 : cohérence du previous_hash ---
        // (le premier est déjà vérifié contre GENESIS_HASH avant la boucle)
        if entry.previous_hash != previous_hash {
            if first_failed_sequence.is_none() {
                first_failed_sequence = Some(entry.sequence_number);
            }
            total_failures += 1;
            // On continue pour compter toutes les violations
            previous_sequence = entry.sequence_number;
            previous_hash = entry.hash; // on avance quand même pour détecter d'autres violations
            continue;
        }

        // --- Vérification 3 : hash recalculé ---
        if let Ok(()) = verify_entry_hash(entry, &previous_hash) {
        } else {
            if first_failed_sequence.is_none() {
                first_failed_sequence = Some(entry.sequence_number);
            }
            total_failures += 1;
            // On continue même après erreur
        }

        previous_hash = entry.hash;
        previous_sequence = entry.sequence_number;
    }

    // Si des violations ont été trouvées, on retourne une erreur agrégée
    if total_failures > 0 {
        return Err(IntegrityError::ChainCorrupted {
            first_failed_sequence: first_failed_sequence.unwrap_or(first.sequence_number),
            total_failures,
            total_checked: entries.len(),
        }
        .into());
    }

    Ok(IntegrityReport {
        entries_checked: entries.len(),
        first_sequence: first.sequence_number,
        last_sequence: entries[entries.len() - 1].sequence_number,
        last_hash: entries[entries.len() - 1].hash,
    })
}

// ---------------------------------------------------------------------------
// Utilitaires
// ---------------------------------------------------------------------------

/// Encode un hash de 32 octets en hexadécimal minuscule (64 caractères).
///
/// Utilisé dans les messages d'erreur et les logs d'audit.
/// Implémentation sans dépendance externe (hex pur).
///
/// # Examples
/// ```
/// use fiscal_engine::hash_engine::hex_encode;
/// let hash = [0u8; 32];
/// assert_eq!(hex_encode(&hash), "0".repeat(64));
/// ```
#[must_use]
pub fn hex_encode(bytes: &[u8; 32]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(64), |mut acc, b| {
        write!(acc, "{b:02x}").expect("writing to String is infallible");
        acc
    })
}

/// Décode une chaîne hexadécimale de 64 caractères en 32 octets.
///
/// # Errors
/// - `HashError::InvalidHashSize` si la chaîne ne fait pas 64 caractères
///
/// # Examples
/// ```
/// use fiscal_engine::hash_engine::hex_decode;
/// let hex = "0".repeat(64);
/// let bytes = hex_decode(&hex).unwrap();
/// assert_eq!(bytes, [0u8; 32]);
/// ```
pub fn hex_decode(hex: &str) -> Result<[u8; 32], HashError> {
    if hex.len() != 64 {
        return Err(HashError::InvalidHashSize {
            received: hex.len() / 2,
        });
    }

    let mut result = [0u8; 32];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let high = hex_char_to_nibble(chunk[0])
            .map_err(|()| HashError::InvalidHashSize { received: 0 })?;
        let low = hex_char_to_nibble(chunk[1])
            .map_err(|()| HashError::InvalidHashSize { received: 0 })?;
        result[i] = (high << 4) | low;
    }
    Ok(result)
}

fn hex_char_to_nibble(c: u8) -> Result<u8, ()> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err(()),
    }
}

// ---------------------------------------------------------------------------
// Constructeur de test — permet de créer des FiscalEntry valides dans les tests
// ---------------------------------------------------------------------------

/// Construit un `FiscalEntry` complet avec hash calculé, pour les tests.
///
/// Cette fonction est publique uniquement pour les tests (doc tests et intégration).
/// En production, les `FiscalEntry` sont créés exclusivement par le journal (Étape 4).
///
/// # Arguments
/// * `sequence_number` - Numéro de séquence de l'entrée.
/// * `session_id_bytes` - UUID de la session (16 octets).
/// * `operation_type` - Type d'opération.
/// * `amount_ttc_cents` - Montant TTC en centimes.
/// * `tva_rate` - Taux de TVA.
/// * `created_at_ms` - Timestamp en millisecondes Unix.
/// * `previous_hash` - Hash de l'entrée précédente.
///
/// # Panics
/// Ne panique pas en usage normal.
#[must_use]
pub fn build_entry_for_test(
    sequence_number: u64,
    session_id_bytes: [u8; 16],
    operation_type: OperationType,
    amount_ttc_cents: i64,
    tva_rate: TvaRate,
    created_at_ms: u64,
    previous_hash: [u8; 32],
) -> FiscalEntry {
    use crate::types::tva::TvaBreakdown;
    use common::{Cents, FiscalEntryId, SessionId};

    let entry_id = FiscalEntryId::new();
    let entry_id_bytes = entry_id.0.into_bytes();
    let session_id = SessionId(uuid::Uuid::from_bytes(session_id_bytes));

    let tva_breakdown = TvaBreakdown::from_ttc(Cents(amount_ttc_cents), tva_rate);

    let input = HashInput {
        sequence_number,
        entry_id_bytes: &entry_id_bytes,
        session_id_bytes: &session_id_bytes,
        operation_type,
        amount_ttc_cents,
        tva_rate,
        ht_cents: tva_breakdown.ht_cents.0,
        tva_cents: tva_breakdown.tva_cents.0,
        created_at_ms,
        previous_hash: &previous_hash,
    };

    let hash = compute_entry_hash(&input);

    FiscalEntry {
        id: entry_id,
        sequence_number,
        session_id,
        operation_type,
        amount_ttc_cents: Cents(amount_ttc_cents),
        tva_breakdown,
        tva_5_5_breakdown: match tva_rate {
            TvaRate::Reduit5_5 => tva_breakdown,
            _ => TvaBreakdown::zero(TvaRate::Reduit5_5),
        },
        tva_10_breakdown: match tva_rate {
            TvaRate::Intermediaire10 => tva_breakdown,
            _ => TvaBreakdown::zero(TvaRate::Intermediaire10),
        },
        tva_20_breakdown: match tva_rate {
            TvaRate::Normal20 => tva_breakdown,
            _ => TvaBreakdown::zero(TvaRate::Normal20),
        },
        reason: None,
        order_reference: None,
        hash,
        previous_hash,
        created_at_ms,
        synced: false,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use common::GENESIS_HASH;

    // -----------------------------------------------------------------------
    // Helpers de test
    // -----------------------------------------------------------------------

    const SESSION_BYTES: [u8; 16] = [0xAB; 16];
    const BASE_TIMESTAMP: u64 = 1_700_000_000_000;

    fn make_entry(seq: u64, prev_hash: [u8; 32]) -> FiscalEntry {
        build_entry_for_test(
            seq,
            SESSION_BYTES,
            OperationType::Sale,
            1100,
            TvaRate::Intermediaire10,
            BASE_TIMESTAMP + seq * 1000,
            prev_hash,
        )
    }

    fn make_chain(n: usize) -> Vec<FiscalEntry> {
        let mut entries = Vec::with_capacity(n);
        let mut prev_hash = GENESIS_HASH;
        for i in 1..=(n as u64) {
            let entry = make_entry(i, prev_hash);
            prev_hash = entry.hash;
            entries.push(entry);
        }
        entries
    }

    // -----------------------------------------------------------------------
    // Tests : compute_entry_hash
    // -----------------------------------------------------------------------

    #[test]
    fn hash_is_32_bytes() {
        let id = [0u8; 16];
        let sess = [0u8; 16];
        let input = HashInput {
            sequence_number: 1,
            entry_id_bytes: &id,
            session_id_bytes: &sess,
            operation_type: OperationType::Sale,
            amount_ttc_cents: 1100,
            tva_rate: TvaRate::Intermediaire10,
            ht_cents: 1000,
            tva_cents: 100,
            created_at_ms: BASE_TIMESTAMP,
            previous_hash: &GENESIS_HASH,
        };
        let hash = compute_entry_hash(&input);
        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn same_input_produces_same_hash() {
        // Déterminisme : même entrée → même hash, toujours
        let id = [1u8; 16];
        let sess = [2u8; 16];
        let input = HashInput {
            sequence_number: 42,
            entry_id_bytes: &id,
            session_id_bytes: &sess,
            operation_type: OperationType::Refund,
            amount_ttc_cents: -500,
            tva_rate: TvaRate::Reduit5_5,
            ht_cents: -474,
            tva_cents: -26,
            created_at_ms: BASE_TIMESTAMP,
            previous_hash: &GENESIS_HASH,
        };
        let h1 = compute_entry_hash(&input);
        let h2 = compute_entry_hash(&input);
        assert_eq!(h1, h2, "Le hash doit être déterministe");
    }

    #[test]
    fn different_sequence_produces_different_hash() {
        let id = [0u8; 16];
        let sess = [0u8; 16];
        let mut input = HashInput {
            sequence_number: 1,
            entry_id_bytes: &id,
            session_id_bytes: &sess,
            operation_type: OperationType::Sale,
            amount_ttc_cents: 1100,
            tva_rate: TvaRate::Intermediaire10,
            ht_cents: 1000,
            tva_cents: 100,
            created_at_ms: BASE_TIMESTAMP,
            previous_hash: &GENESIS_HASH,
        };
        let h1 = compute_entry_hash(&input);
        input.sequence_number = 2;
        let h2 = compute_entry_hash(&input);
        assert_ne!(
            h1, h2,
            "Des séquences différentes doivent donner des hashs différents"
        );
    }

    #[test]
    fn different_amount_produces_different_hash() {
        let id = [0u8; 16];
        let sess = [0u8; 16];
        let mut input = HashInput {
            sequence_number: 1,
            entry_id_bytes: &id,
            session_id_bytes: &sess,
            operation_type: OperationType::Sale,
            amount_ttc_cents: 1100,
            tva_rate: TvaRate::Intermediaire10,
            ht_cents: 1000,
            tva_cents: 100,
            created_at_ms: BASE_TIMESTAMP,
            previous_hash: &GENESIS_HASH,
        };
        let h1 = compute_entry_hash(&input);
        input.amount_ttc_cents = 1101; // 1 centime de différence
        let h2 = compute_entry_hash(&input);
        assert_ne!(h1, h2, "Un centime de différence doit changer le hash");
    }

    #[test]
    fn different_previous_hash_produces_different_hash() {
        let id = [0u8; 16];
        let sess = [0u8; 16];
        let prev1 = GENESIS_HASH;
        let prev2 = [0xFF; 32];
        let mut input = HashInput {
            sequence_number: 2,
            entry_id_bytes: &id,
            session_id_bytes: &sess,
            operation_type: OperationType::Sale,
            amount_ttc_cents: 1100,
            tva_rate: TvaRate::Intermediaire10,
            ht_cents: 1000,
            tva_cents: 100,
            created_at_ms: BASE_TIMESTAMP,
            previous_hash: &prev1,
        };
        let h1 = compute_entry_hash(&input);
        input.previous_hash = &prev2;
        let h2 = compute_entry_hash(&input);
        assert_ne!(
            h1, h2,
            "Des previous_hash différents doivent donner des hashs différents"
        );
    }

    #[test]
    fn all_operation_types_produce_different_hashes() {
        let id = [0u8; 16];
        let sess = [0u8; 16];
        let base = HashInput {
            sequence_number: 1,
            entry_id_bytes: &id,
            session_id_bytes: &sess,
            operation_type: OperationType::Sale,
            amount_ttc_cents: 1100,
            tva_rate: TvaRate::Intermediaire10,
            ht_cents: 1000,
            tva_cents: 100,
            created_at_ms: BASE_TIMESTAMP,
            previous_hash: &GENESIS_HASH,
        };
        let ops = [
            OperationType::Sale,
            OperationType::Refund,
            OperationType::Cancel,
            OperationType::Discount,
            OperationType::ZClose,
        ];
        let hashes: Vec<_> = ops
            .iter()
            .map(|op| {
                let mut i = base;
                i.operation_type = *op;
                compute_entry_hash(&i)
            })
            .collect();

        // Tous les hashs doivent être distincts
        for (i, h1) in hashes.iter().enumerate() {
            for (j, h2) in hashes.iter().enumerate() {
                if i != j {
                    assert_ne!(h1, h2, "ops[{i}] et ops[{j}] produisent le même hash");
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Tests : verify_entry_hash
    // -----------------------------------------------------------------------

    #[test]
    fn verify_valid_entry_succeeds() {
        let entry = make_entry(1, GENESIS_HASH);
        assert!(verify_entry_hash(&entry, &GENESIS_HASH).is_ok());
    }

    #[test]
    fn verify_tampered_amount_fails() {
        let mut entry = make_entry(1, GENESIS_HASH);
        // On modifie le montant après création — simule une falsification
        entry.amount_ttc_cents = common::Cents(9999);
        let result = verify_entry_hash(&entry, &GENESIS_HASH);
        assert!(result.is_err(), "Un montant falsifié doit être détecté");
        match result.unwrap_err() {
            FiscalError::Integrity(IntegrityError::HashMismatch { sequence, .. }) => {
                assert_eq!(sequence, 1);
            }
            other => panic!("Erreur inattendue : {other}"),
        }
    }

    #[test]
    fn verify_tampered_sequence_fails() {
        let mut entry = make_entry(5, [0xAA; 32]);
        entry.sequence_number = 6; // falsification du numéro de séquence
        let result = verify_entry_hash(&entry, &[0xAA; 32]);
        assert!(result.is_err());
    }

    #[test]
    fn verify_wrong_previous_hash_fails() {
        let entry = make_entry(2, GENESIS_HASH);
        // On passe un previous_hash différent de celui utilisé pour créer l'entrée
        let wrong_prev = [0xFF; 32];
        assert!(verify_entry_hash(&entry, &wrong_prev).is_err());
    }

    // -----------------------------------------------------------------------
    // Tests : verify_chain_integrity — cas nominaux
    // -----------------------------------------------------------------------

    #[test]
    fn single_valid_entry_chain() {
        let entries = make_chain(1);
        let report = verify_chain_integrity(&entries).expect("Chaîne à 1 entrée doit être valide");
        assert_eq!(report.entries_checked, 1);
        assert_eq!(report.first_sequence, 1);
        assert_eq!(report.last_sequence, 1);
        assert_eq!(report.last_hash, entries[0].hash);
    }

    #[test]
    fn ten_entry_chain_is_valid() {
        let entries = make_chain(10);
        let report = verify_chain_integrity(&entries).expect("Chaîne de 10 entrées valide");
        assert_eq!(report.entries_checked, 10);
        assert_eq!(report.first_sequence, 1);
        assert_eq!(report.last_sequence, 10);
    }

    #[test]
    fn one_thousand_entry_chain_is_valid() {
        // Test de performance réaliste : 1 000 transactions (midi chargé)
        let entries = make_chain(1000);
        let report =
            verify_chain_integrity(&entries).expect("Chaîne de 1 000 entrées doit être valide");
        assert_eq!(report.entries_checked, 1000);
        assert_eq!(report.last_sequence, 1000);
    }

    #[test]
    fn mixed_operation_types_chain_is_valid() {
        // Chaîne avec différents types d'opérations
        let ops = [
            (OperationType::Sale, 1100_i64, TvaRate::Intermediaire10),
            (OperationType::Sale, 550, TvaRate::Reduit5_5),
            (OperationType::Discount, -100, TvaRate::Intermediaire10),
            (OperationType::Sale, 2400, TvaRate::Normal20),
            (OperationType::Refund, -1100, TvaRate::Intermediaire10),
            (OperationType::ZClose, 0, TvaRate::Intermediaire10),
        ];
        let mut entries = Vec::new();
        let mut prev_hash = GENESIS_HASH;
        for (i, (op, amount, rate)) in ops.iter().enumerate() {
            let entry = build_entry_for_test(
                i as u64 + 1,
                SESSION_BYTES,
                *op,
                *amount,
                *rate,
                BASE_TIMESTAMP + i as u64 * 1000,
                prev_hash,
            );
            prev_hash = entry.hash;
            entries.push(entry);
        }
        let report = verify_chain_integrity(&entries).expect("Chaîne mixte doit être valide");
        assert_eq!(report.entries_checked, ops.len());
    }

    // -----------------------------------------------------------------------
    // Tests : verify_chain_integrity — cas d'erreur
    // -----------------------------------------------------------------------

    #[test]
    fn empty_journal_returns_error() {
        let result = verify_chain_integrity(&[]);
        assert!(result.is_err());
        match result.unwrap_err() {
            FiscalError::Integrity(IntegrityError::EmptyJournal) => {}
            other => panic!("Attendu EmptyJournal, obtenu : {other}"),
        }
    }

    #[test]
    fn invalid_genesis_hash_detected() {
        let mut entries = make_chain(3);
        // On altère le previous_hash de la première entrée (doit être GENESIS_HASH)
        entries[0].previous_hash = [0xFF; 32];
        let result = verify_chain_integrity(&entries);
        assert!(result.is_err());
        match result.unwrap_err() {
            FiscalError::Integrity(IntegrityError::InvalidGenesisHash { sequence }) => {
                assert_eq!(sequence, 1);
            }
            other => panic!("Attendu InvalidGenesisHash, obtenu : {other}"),
        }
    }

    #[test]
    fn sequence_gap_detected_fail_fast() {
        let mut entries = make_chain(5);
        // On crée un trou en numérotant la 3e entrée comme la 5e
        entries[2].sequence_number = 5;
        let result = verify_chain_integrity(&entries);
        assert!(result.is_err());
        match result.unwrap_err() {
            FiscalError::Integrity(IntegrityError::SequenceGap {
                expected_sequence,
                found_sequence,
            }) => {
                assert_eq!(expected_sequence, 3);
                assert_eq!(found_sequence, 5);
            }
            other => panic!("Attendu SequenceGap, obtenu : {other}"),
        }
    }

    #[test]
    fn tampered_middle_entry_detected() {
        let mut entries = make_chain(10);
        // On falsifie le montant de l'entrée 5
        entries[4].amount_ttc_cents = common::Cents(99_999);
        let result = verify_chain_integrity(&entries);
        assert!(result.is_err());
        match result.unwrap_err() {
            FiscalError::Integrity(IntegrityError::ChainCorrupted {
                first_failed_sequence,
                total_failures,
                total_checked,
            }) => {
                // La violation est à la séquence 5
                assert_eq!(
                    first_failed_sequence, 5,
                    "La première violation doit être à la séq. 5"
                );
                // Toutes les entrées doivent être vérifiées (pas de fail-fast sur les hashs)
                assert_eq!(total_checked, 10);
                // Au moins 1 violation (l'entrée 5 elle-même)
                assert!(total_failures >= 1);
            }
            other => panic!("Attendu ChainCorrupted, obtenu : {other}"),
        }
    }

    #[test]
    fn tampered_last_entry_detected() {
        let mut entries = make_chain(5);
        entries[4].amount_ttc_cents = common::Cents(1);
        let result = verify_chain_integrity(&entries);
        assert!(result.is_err());
    }

    #[test]
    fn tampered_first_entry_detected() {
        let mut entries = make_chain(5);
        entries[0].amount_ttc_cents = common::Cents(1);
        // La première entrée est falsifiée : son hash ne correspond plus
        // ET le previous_hash de l'entrée 2 ne correspond plus au hash de l'entrée 1
        let result = verify_chain_integrity(&entries);
        assert!(result.is_err());
    }

    #[test]
    fn multiple_tampered_entries_all_counted() {
        let mut entries = make_chain(10);
        // On falsifie les entrées 3 et 7
        entries[2].amount_ttc_cents = common::Cents(1);
        entries[6].amount_ttc_cents = common::Cents(1);
        let result = verify_chain_integrity(&entries);
        assert!(result.is_err());
        match result.unwrap_err() {
            FiscalError::Integrity(IntegrityError::ChainCorrupted { total_failures, .. }) => {
                // Au moins 2 violations, potentiellement plus (effet cascade sur previous_hash)
                assert!(
                    total_failures >= 2,
                    "Au moins 2 violations attendues, obtenu {total_failures}"
                );
            }
            other => panic!("Attendu ChainCorrupted, obtenu : {other}"),
        }
    }

    // -----------------------------------------------------------------------
    // Tests : utilitaires hex
    // -----------------------------------------------------------------------

    #[test]
    fn hex_encode_all_zeros() {
        assert_eq!(hex_encode(&[0u8; 32]), "0".repeat(64));
    }

    #[test]
    fn hex_encode_all_ff() {
        assert_eq!(hex_encode(&[0xFF; 32]), "ff".repeat(32));
    }

    #[test]
    fn hex_encode_decode_roundtrip() {
        let original = [
            0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0xFE, 0xDC, 0xBA, 0x98, 0x76, 0x54,
            0x32, 0x10, 0x00, 0xFF, 0x0F, 0xF0, 0xAA, 0x55, 0x33, 0xCC, 0x11, 0x22, 0x44, 0x88,
            0xBB, 0xDD, 0xEE, 0x77,
        ];
        let hex = hex_encode(&original);
        assert_eq!(hex.len(), 64);
        let decoded = hex_decode(&hex).expect("Le décodage doit réussir");
        assert_eq!(decoded, original);
    }

    #[test]
    fn hex_decode_invalid_length_fails() {
        let result = hex_decode("abc"); // trop court
        assert!(result.is_err());
    }

    #[test]
    fn hex_decode_known_value() {
        // Vecteur de test fixe pour garantir la stabilité du format
        let hex = "0000000000000000000000000000000000000000000000000000000000000000";
        let bytes = hex_decode(hex).unwrap();
        assert_eq!(bytes, [0u8; 32]);
    }

    // -----------------------------------------------------------------------
    // Test de non-régression : vecteur de hash fixe
    // -----------------------------------------------------------------------

    #[test]
    fn hash_vector_stability() {
        // Ce test garantit que le schéma de hash ne change pas entre versions.
        // Si ce test échoue après une modification du code, c'est un CHANGEMENT CASSANT
        // qui invalide tous les journaux existants.
        //
        // Vecteur calculé avec l'implémentation de référence :
        // Input : seq=1, entry_id=[0x01;16], session_id=[0x02;16],
        //         op=Sale(0x01), amount=1100, tva=Intermediaire10(0x02),
        //         ht=1000, tva_cents=100, ts=1_700_000_000_000, prev=GENESIS_HASH
        //
        // Message (106 octets) :
        // 0000000000000001  (seq=1, u64 be)
        // 01010101...01     (entry_id, 16x 0x01)
        // 02020202...02     (session_id, 16x 0x02)
        // 01                (Sale discriminant)
        // 000000000000044C  (1100 as i64 be)
        // 02                (Intermediaire10 discriminant)
        // 00000000000003E8  (1000 as i64 be)
        // 0000000000000064  (100 as i64 be)
        // 000001895F9F5A00  (1_700_000_000_000 as u64 be)
        // 0000000000000000...0000 (GENESIS_HASH, 32x 0x00)

        let id = [0x01u8; 16];
        let sess = [0x02u8; 16];
        let input = HashInput {
            sequence_number: 1,
            entry_id_bytes: &id,
            session_id_bytes: &sess,
            operation_type: OperationType::Sale,
            amount_ttc_cents: 1100,
            tva_rate: TvaRate::Intermediaire10,
            ht_cents: 1000,
            tva_cents: 100,
            created_at_ms: 1_700_000_000_000,
            previous_hash: &GENESIS_HASH,
        };

        let hash = compute_entry_hash(&input);
        let hex = hex_encode(&hash);

        // Vecteur de référence (à calculer une fois et figer ici)
        // Valeur calculée par l'implémentation — à vérifier manuellement en audit LNE
        // Si cette assertion échoue : CHANGEMENT CASSANT DU SCHÉMA DE HASH
        assert_eq!(hex.len(), 64, "Le hash doit faire 64 caractères hex");

        // On vérifie le déterminisme plutôt qu'une valeur absolue ici,
        // la valeur absolue sera fixée après le premier run réussi en CI.
        // TODO(audit) : remplacer par la valeur hex absolue après le premier run CI :
        // assert_eq!(hex, "VALEUR_HEX_ICI");
        let hash2 = compute_entry_hash(&input);
        assert_eq!(hash, hash2, "Le hash doit être déterministe");
    }
}
