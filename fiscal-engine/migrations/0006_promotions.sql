-- fiscal-engine/migrations/0006_promotions.sql
-- Table locale des promotions actives (sync-client → edge-api, lecture seule côté caisse)
CREATE TABLE IF NOT EXISTS promotions (
  id               TEXT PRIMARY KEY,
  name             TEXT NOT NULL,
  promo_type       TEXT NOT NULL,
  value_cents      INTEGER,
  value_bps        INTEGER,
  target_sku       TEXT,
  trigger_type     TEXT NOT NULL,           -- 'auto' | 'manual' (renamed from trigger: SQLite reserved keyword)
  exclusion_group  TEXT,
  priority         INTEGER NOT NULL DEFAULT 0,
  valid_from       TEXT,                    -- ISO date YYYY-MM-DD ou NULL
  valid_to         TEXT,
  days_of_week     TEXT,                    -- JSON array ex: "[1,2,3]" ou NULL
  time_from        TEXT,                    -- HH:MM ou NULL
  time_to          TEXT,
  active           INTEGER NOT NULL DEFAULT 0,  -- 1 = active (miroir du champ Supabase)
  updated_at_ms    INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_promotions_updated_at ON promotions (updated_at_ms);
