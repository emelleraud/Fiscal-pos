//! # archive
//!
//! Types pour l'archivage annuel signé des données fiscales.
//!
//! ## Obligation légale (NF525 §7 + CGI art. 286)
//! Les logiciels de caisse certifiés NF525 doivent produire chaque année
//! un fichier d'archive contenant toutes les transactions de l'exercice,
//! signé numériquement pour garantir l'authenticité.
//!
//! ## Format de l'archive
//! - Fichier CSV avec séparateur `;` (norme française)
//! - Encodage UTF-8 avec BOM (compatibilité Excel française)
//! - Une ligne par `FiscalEntry`, dans l'ordre chronologique
//! - Signature Ed25519 sur le hash SHA-256 du fichier CSV complet
//!
//! ## Colonnes CSV (ordre fixe, NF525 §7.3)
//! ```text
//! sequence;id;session_id;operation_type;amount_ttc_cents;ht_cents;
//! tva_cents;tva_rate;hash_hex;previous_hash_hex;created_at_ms;reason
//! ```

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Sérialise un tableau de 64 octets en string hexadécimale.
fn serialize_bytes64<S: Serializer>(bytes: &[u8; 64], s: S) -> Result<S::Ok, S::Error> {
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    s.serialize_str(&hex)
}

/// Désérialise une string hexadécimale en tableau de 64 octets.
fn deserialize_bytes64<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 64], D::Error> {
    let hex = String::deserialize(d)?;
    if hex.len() != 128 {
        return Err(serde::de::Error::custom(format!(
            "longueur hex invalide : attendu 128, reçu {}",
            hex.len()
        )));
    }
    let mut result = [0u8; 64];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let hi = hex_nibble(chunk[0]).map_err(serde::de::Error::custom)?;
        let lo = hex_nibble(chunk[1]).map_err(serde::de::Error::custom)?;
        result[i] = (hi << 4) | lo;
    }
    Ok(result)
}

fn hex_nibble(c: u8) -> Result<u8, &'static str> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err("caractère hexadécimal invalide"),
    }
}

/// Export annuel des données fiscales, avant signature.
///
/// Contient le contenu CSV brut et les métadonnées nécessaires à la signature.
/// La signature est ajoutée par `sign_archive()` pour produire un `SignedArchive`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveExport {
    /// Année fiscale de l'archive (ex: 2024).
    pub year: u32,

    /// Nombre d'entrées fiscales dans l'archive.
    pub entry_count: u64,

    /// Nombre de sessions de caisse couvertes.
    pub session_count: u64,

    /// Numéro de séquence de la première entrée.
    pub first_sequence: u64,

    /// Numéro de séquence de la dernière entrée.
    pub last_sequence: u64,

    /// Timestamp de génération en millisecondes Unix (UTC).
    pub generated_at_ms: u64,

    /// Contenu CSV complet de l'archive (UTF-8 avec BOM).
    /// Ce champ peut être volumineux (> 1 Mo pour une grande chaîne).
    pub csv_content: Vec<u8>,

    /// Hash SHA-256 du `csv_content`, calculé avant signature.
    /// Sert de message signé pour Ed25519.
    pub csv_hash: [u8; 32],
}

/// Archive annuelle signée, prête pour dépôt légal.
///
/// Produite par `sign_archive()` à partir d'un `ArchiveExport`.
/// La signature Ed25519 couvre le `csv_hash` de l'export.
///
/// ## Vérification
/// Un auditeur LNE peut vérifier la signature en :
/// 1. Calculant le SHA-256 du fichier CSV
/// 2. Vérifiant la signature Ed25519 avec la clé publique du logiciel (enregistrée lors de la certification)
///
/// # Examples
/// ```
/// use fiscal_engine::types::archive::SignedArchive;
/// // La création se fait via Journal::sign_archive() (Étape 5)
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedArchive {
    /// L'export non-signé sous-jacent.
    pub export: ArchiveExport,

    /// Signature Ed25519 (64 octets) du `export.csv_hash`.
    #[serde(
        serialize_with = "serialize_bytes64",
        deserialize_with = "deserialize_bytes64"
    )]
    pub signature: [u8; 64],

    /// Clé publique Ed25519 (32 octets) correspondant à la clé de signature.
    /// Enregistrée lors de la certification LNE.
    pub public_key: [u8; 32],

    /// Version du logiciel au moment de la génération (ex: "1.0.0").
    /// Doit correspondre à la version déclarée lors de la certification.
    pub software_version: String,

    /// Identifiant du site fiscal (SIRET ou identifiant réseau interne).
    pub site_id: String,
}

impl SignedArchive {
    /// Vérifie que les métadonnées de l'archive sont cohérentes.
    ///
    /// Ne vérifie **pas** la signature cryptographique (nécessite la clé publique
    /// et la bibliothèque `ed25519-dalek` — fait dans le journal à l'Étape 5).
    ///
    /// Vérifie :
    /// - L'année est dans une plage raisonnable (2020–2100)
    /// - Le nombre d'entrées est > 0
    /// - `first_sequence <= last_sequence`
    /// - Le `csv_content` n'est pas vide
    /// - Le `software_version` n'est pas vide
    ///
    /// # Errors
    /// Retourne une description de l'incohérence.
    pub fn verify_metadata(&self) -> Result<(), String> {
        let e = &self.export;

        if e.year < 2020 || e.year > 2100 {
            return Err(format!("Année invalide : {}", e.year));
        }

        if e.entry_count == 0 {
            return Err("L'archive ne contient aucune entrée".to_string());
        }

        if e.first_sequence > e.last_sequence {
            return Err(format!(
                "Séquence invalide : first={} > last={}",
                e.first_sequence, e.last_sequence
            ));
        }

        if e.csv_content.is_empty() {
            return Err("Le contenu CSV est vide".to_string());
        }

        if self.software_version.is_empty() {
            return Err("La version du logiciel est manquante".to_string());
        }

        if self.site_id.is_empty() {
            return Err("L'identifiant de site est manquant".to_string());
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_export() -> ArchiveExport {
        ArchiveExport {
            year: 2024,
            entry_count: 1000,
            session_count: 30,
            first_sequence: 1,
            last_sequence: 1000,
            generated_at_ms: 1_700_000_000_000,
            csv_content: b"sequence;id;...".to_vec(),
            csv_hash: [0u8; 32],
        }
    }

    fn sample_signed(export: ArchiveExport) -> SignedArchive {
        SignedArchive {
            export,
            signature: [0u8; 64],
            public_key: [0u8; 32],
            software_version: "1.0.0".to_string(),
            site_id: "12345678900000".to_string(),
        }
    }

    #[test]
    fn valid_archive_passes_metadata_check() {
        let signed = sample_signed(sample_export());
        assert!(signed.verify_metadata().is_ok());
    }

    #[test]
    fn invalid_year_fails_metadata_check() {
        let mut export = sample_export();
        export.year = 1999;
        let signed = sample_signed(export);
        let err = signed.verify_metadata().unwrap_err();
        assert!(err.contains("1999"));
    }

    #[test]
    fn zero_entries_fails_metadata_check() {
        let mut export = sample_export();
        export.entry_count = 0;
        let signed = sample_signed(export);
        assert!(signed.verify_metadata().is_err());
    }

    #[test]
    fn invalid_sequence_range_fails_metadata_check() {
        let mut export = sample_export();
        export.first_sequence = 500;
        export.last_sequence = 100; // incohérent
        let signed = sample_signed(export);
        assert!(signed.verify_metadata().is_err());
    }

    #[test]
    fn empty_csv_fails_metadata_check() {
        let mut export = sample_export();
        export.csv_content = vec![];
        let signed = sample_signed(export);
        assert!(signed.verify_metadata().is_err());
    }

    #[test]
    fn empty_software_version_fails_metadata_check() {
        let mut signed = sample_signed(sample_export());
        signed.software_version = String::new();
        assert!(signed.verify_metadata().is_err());
    }

    #[test]
    fn empty_site_id_fails_metadata_check() {
        let mut signed = sample_signed(sample_export());
        signed.site_id = String::new();
        assert!(signed.verify_metadata().is_err());
    }
}
