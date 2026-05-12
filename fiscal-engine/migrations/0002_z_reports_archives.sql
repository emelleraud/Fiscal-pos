-- =============================================================================
-- Migration 0002 : Table z_reports
--
-- Le rapport Z est immuable après génération (NF525 §6).
-- Une seule ligne par session (contrainte UNIQUE sur session_id).
-- =============================================================================

CREATE TABLE IF NOT EXISTS z_reports (
    -- Identifiant unique du rapport Z
    id                      TEXT        NOT NULL PRIMARY KEY,

    -- Session clôturée
    session_id              TEXT        NOT NULL UNIQUE REFERENCES sessions(id),
    session_sequence_number INTEGER     NOT NULL CHECK (session_sequence_number >= 1),

    -- Timestamp de génération (immuable)
    generated_at_ms         INTEGER     NOT NULL,

    -- Plage de séquences couverte
    first_entry_sequence    INTEGER     NOT NULL CHECK (first_entry_sequence >= 1),
    last_entry_sequence     INTEGER     NOT NULL,
    entry_count             INTEGER     NOT NULL CHECK (entry_count >= 1),

    -- Totaux financiers (centimes entiers, toujours positifs)
    total_sales_cents       INTEGER     NOT NULL DEFAULT 0 CHECK (total_sales_cents >= 0),
    total_refunds_cents     INTEGER     NOT NULL DEFAULT 0 CHECK (total_refunds_cents >= 0),
    total_cancels_cents     INTEGER     NOT NULL DEFAULT 0 CHECK (total_cancels_cents >= 0),
    total_discounts_cents   INTEGER     NOT NULL DEFAULT 0 CHECK (total_discounts_cents >= 0),

    -- Décomposition TVA par taux (HT + TVA + TTC stockés séparément)
    tva_5_5_ht_cents        INTEGER     NOT NULL DEFAULT 0,
    tva_5_5_tva_cents       INTEGER     NOT NULL DEFAULT 0,
    tva_5_5_ttc_cents       INTEGER     NOT NULL DEFAULT 0,

    tva_10_ht_cents         INTEGER     NOT NULL DEFAULT 0,
    tva_10_tva_cents        INTEGER     NOT NULL DEFAULT 0,
    tva_10_ttc_cents        INTEGER     NOT NULL DEFAULT 0,

    tva_20_ht_cents         INTEGER     NOT NULL DEFAULT 0,
    tva_20_tva_cents        INTEGER     NOT NULL DEFAULT 0,
    tva_20_ttc_cents        INTEGER     NOT NULL DEFAULT 0,

    -- Hash de clôture (32 octets — dernière entrée ZClose de la session)
    closing_hash            BLOB        NOT NULL CHECK (length(closing_hash) = 32),

    -- Cohérence des séquences
    CHECK (last_entry_sequence >= first_entry_sequence)
);

-- Index pour la recherche par session (usage : edge-api GET /sessions/current)
CREATE INDEX IF NOT EXISTS idx_z_reports_session
    ON z_reports (session_id);

-- =============================================================================
-- Migration 0002 : Table archive_metadata
--
-- Trace les archives annuelles générées pour éviter les doublons.
-- Le CSV lui-même est stocké sur disque (chemin dans csv_path).
-- =============================================================================

CREATE TABLE IF NOT EXISTS archive_metadata (
    -- Année fiscale (clé primaire : une archive par an)
    year                    INTEGER     NOT NULL PRIMARY KEY CHECK (year >= 2020),

    -- Métadonnées de contenu
    entry_count             INTEGER     NOT NULL CHECK (entry_count > 0),
    session_count           INTEGER     NOT NULL CHECK (session_count > 0),
    first_sequence          INTEGER     NOT NULL CHECK (first_sequence >= 1),
    last_sequence           INTEGER     NOT NULL,

    -- Timestamp de génération
    generated_at_ms         INTEGER     NOT NULL,

    -- Hash SHA-256 du CSV (hex, 64 chars) — sert à vérifier l'intégrité du fichier sur disque
    csv_hash_hex            TEXT        NOT NULL CHECK (length(csv_hash_hex) = 64),

    -- Signature Ed25519 (hex, 128 chars)
    signature_hex           TEXT        NOT NULL CHECK (length(signature_hex) = 128),

    -- Clé publique Ed25519 (hex, 64 chars)
    public_key_hex          TEXT        NOT NULL CHECK (length(public_key_hex) = 64),

    -- Informations logiciel
    software_version        TEXT        NOT NULL,
    site_id                 TEXT        NOT NULL,

    -- Chemin du fichier CSV sur disque (chemin absolu ou relatif à la base)
    csv_path                TEXT        NOT NULL,

    CHECK (last_sequence >= first_sequence)
);
