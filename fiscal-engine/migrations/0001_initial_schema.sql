-- =============================================================================
-- Migration 0001 : Schéma initial du journal fiscal NF525
--
-- Contraintes d'architecture :
-- * Aucun UPDATE ni DELETE ne doit jamais être exécuté sur fiscal_entries.
--   L'append-only est garanti par Rust (pas de méthodes mutantes) et renforcé
--   ici par l'absence de trigger d'update, mais NE PAS ajouter de politique
--   RESTRICT sur la base elle-même car sqlx ne le supporte pas facilement.
-- * Les montants sont en centimes entiers (INTEGER signé 64 bits = i64 Rust).
-- * Les hash sont stockés en BLOB de 32 octets (pas en hex TEXT : économie d'espace
--   et élimination des conversions à la lecture).
-- * WAL mode activé dans le code applicatif (pas ici : PRAGMA ne persiste pas
--   dans une migration sqlx).
-- =============================================================================

-- -----------------------------------------------------------------------------
-- Table : sessions
-- Une ligne par session de caisse (entre deux rapports Z).
-- -----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS sessions (
    -- Identifiant UUID v7 (16 octets bruts stockés en TEXT pour lisibilité)
    id                      TEXT        NOT NULL PRIMARY KEY,

    -- Numéro de séquence de session, commence à 1, sans trou
    sequence_number         INTEGER     NOT NULL UNIQUE CHECK (sequence_number >= 1),

    -- État : 'Open' ou 'Closed'
    status                  TEXT        NOT NULL DEFAULT 'Open'
                                        CHECK (status IN ('Open', 'Closed')),

    -- Timestamps en millisecondes Unix (UTC)
    opened_at_ms            INTEGER     NOT NULL,
    closed_at_ms            INTEGER,                    -- NULL si session ouverte

    -- Hash SHA-256 de la dernière entrée (ZClose) — NULL si session ouverte
    closing_hash            BLOB,                       -- 32 octets

    -- Accumulateurs de totaux (centimes entiers, valeurs absolues pour refunds/cancels)
    total_sales_cents       INTEGER     NOT NULL DEFAULT 0,
    total_refunds_cents     INTEGER     NOT NULL DEFAULT 0,
    total_cancels_cents     INTEGER     NOT NULL DEFAULT 0,
    total_discounts_cents   INTEGER     NOT NULL DEFAULT 0,
    entry_count             INTEGER     NOT NULL DEFAULT 0,

    -- Contraintes de cohérence
    CHECK (total_sales_cents    >= 0),
    CHECK (total_refunds_cents  >= 0),
    CHECK (total_cancels_cents  >= 0),
    CHECK (total_discounts_cents >= 0),
    CHECK (entry_count          >= 0),

    -- Si la session est fermée, closed_at_ms et closing_hash doivent être renseignés
    CHECK (
        (status = 'Open'   AND closed_at_ms IS NULL AND closing_hash IS NULL) OR
        (status = 'Closed' AND closed_at_ms IS NOT NULL AND closing_hash IS NOT NULL)
    )
);

-- -----------------------------------------------------------------------------
-- Table : fiscal_entries — LE JOURNAL IMMUABLE
--
-- Règle absolue : INSERT uniquement. Aucun UPDATE, aucun DELETE.
-- NF525 §4.1 : toute tentative de modification doit être rejetée.
-- -----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS fiscal_entries (
    -- Identifiant UUID v7 (TEXT pour lisibilité dans les logs)
    id                      TEXT        NOT NULL PRIMARY KEY,

    -- Numéro de séquence global, commence à 1, continu sans trou
    sequence_number         INTEGER     NOT NULL UNIQUE CHECK (sequence_number >= 1),

    -- Session parente (FK sur sessions.id)
    session_id              TEXT        NOT NULL REFERENCES sessions(id),

    -- Type d'opération : 'Sale', 'Refund', 'Cancel', 'Discount', 'ZClose'
    operation_type          TEXT        NOT NULL
                                        CHECK (operation_type IN (
                                            'Sale', 'Refund', 'Cancel', 'Discount', 'ZClose'
                                        )),

    -- Montants en centimes (signés : positif pour ventes, négatif pour remboursements)
    amount_ttc_cents        INTEGER     NOT NULL,
    ht_cents                INTEGER     NOT NULL,
    tva_cents               INTEGER     NOT NULL,

    -- Taux de TVA applicable : '5.5', '10', '20'
    tva_rate                TEXT        NOT NULL
                                        CHECK (tva_rate IN ('5.5', '10', '20')),

    -- Chaîne cryptographique (32 octets bruts chacun)
    hash                    BLOB        NOT NULL CHECK (length(hash) = 32),
    previous_hash           BLOB        NOT NULL CHECK (length(previous_hash) = 32),

    -- Timestamp de création en millisecondes Unix (UTC)
    created_at_ms           INTEGER     NOT NULL,

    -- Contexte métier optionnel
    reason                  TEXT,                       -- obligatoire pour Refund/Cancel
    order_reference         TEXT,                       -- référence commande

    -- Statut de synchronisation (géré par sync-client, jamais par fiscal-engine)
    synced                  INTEGER     NOT NULL DEFAULT 0 CHECK (synced IN (0, 1)),

    -- Cohérence signe / type d'opération
    CHECK (
        (operation_type = 'Sale'     AND amount_ttc_cents > 0) OR
        (operation_type = 'Refund'   AND amount_ttc_cents < 0) OR
        (operation_type = 'Cancel'   AND amount_ttc_cents < 0) OR
        (operation_type = 'Discount' AND amount_ttc_cents < 0) OR
        (operation_type = 'ZClose'   AND amount_ttc_cents = 0)
    ),

    -- Motif obligatoire pour Refund et Cancel
    CHECK (
        operation_type NOT IN ('Refund', 'Cancel') OR reason IS NOT NULL
    )
);

-- -----------------------------------------------------------------------------
-- Index — performances des requêtes courantes
-- -----------------------------------------------------------------------------

-- Requête principale sync-client : entrées non synchronisées par ordre de séquence
CREATE INDEX IF NOT EXISTS idx_fiscal_entries_unsynced
    ON fiscal_entries (synced, sequence_number)
    WHERE synced = 0;

-- Requête de vérification d'intégrité : toutes les entrées d'une session
CREATE INDEX IF NOT EXISTS idx_fiscal_entries_session
    ON fiscal_entries (session_id, sequence_number);

-- Requête d'archivage annuel : entrées par année (filtrage sur created_at_ms)
CREATE INDEX IF NOT EXISTS idx_fiscal_entries_created_at
    ON fiscal_entries (created_at_ms);

-- Dernière entrée de la chaîne (pour récupérer le previous_hash rapidement)
CREATE INDEX IF NOT EXISTS idx_fiscal_entries_sequence
    ON fiscal_entries (sequence_number DESC);
