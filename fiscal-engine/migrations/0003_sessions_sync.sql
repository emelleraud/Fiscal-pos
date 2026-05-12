-- =============================================================================
-- Migration 0003 : Ajout du suivi de synchronisation sur les sessions
--
-- Contexte : le sync-client doit pousser les sessions vers Supabase avant
-- les fiscal_entries (contrainte FK session_id → sessions.id côté cloud).
-- On reproduit le même pattern que fiscal_entries.synced.
-- =============================================================================

ALTER TABLE sessions ADD COLUMN synced     INTEGER NOT NULL DEFAULT 0
    CHECK (synced IN (0, 1));

ALTER TABLE sessions ADD COLUMN synced_at  INTEGER;  -- ms Unix UTC, NULL si non synchronisé

-- Index pour la requête sync-client : sessions non synchronisées par séquence
CREATE INDEX IF NOT EXISTS idx_sessions_unsynced
    ON sessions (synced, sequence_number)
    WHERE synced = 0;
