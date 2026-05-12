-- =============================================================================
-- Migration 0004 : Suivi de synchronisation sur z_reports
--
-- Contexte : le sync-client doit pousser les z_reports vers Supabase après
-- les sessions (FK session_id → sessions.id côté cloud).
-- On reproduit le même pattern que fiscal_entries.synced et sessions.synced.
-- =============================================================================

ALTER TABLE z_reports ADD COLUMN synced     INTEGER NOT NULL DEFAULT 0
    CHECK (synced IN (0, 1));

ALTER TABLE z_reports ADD COLUMN synced_at  INTEGER;  -- ms Unix UTC, NULL si non synchronisé

CREATE INDEX IF NOT EXISTS idx_z_reports_unsynced
    ON z_reports (synced)
    WHERE synced = 0;
