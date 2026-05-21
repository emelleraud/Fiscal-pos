-- Migration 0005 : Ajout des colonnes de ventilation TVA par taux (NF525 §4.3.1)
-- Ces colonnes stockent la décomposition HT/TVA pour chaque taux applicable.
-- Le TTC par taux est dérivé en lecture (ht + tva) — pas besoin de le stocker.

ALTER TABLE fiscal_entries ADD COLUMN tva_5_5_ht_cents  INTEGER NOT NULL DEFAULT 0;
ALTER TABLE fiscal_entries ADD COLUMN tva_5_5_tva_cents INTEGER NOT NULL DEFAULT 0;
ALTER TABLE fiscal_entries ADD COLUMN tva_10_ht_cents   INTEGER NOT NULL DEFAULT 0;
ALTER TABLE fiscal_entries ADD COLUMN tva_10_tva_cents  INTEGER NOT NULL DEFAULT 0;
ALTER TABLE fiscal_entries ADD COLUMN tva_20_ht_cents   INTEGER NOT NULL DEFAULT 0;
ALTER TABLE fiscal_entries ADD COLUMN tva_20_tva_cents  INTEGER NOT NULL DEFAULT 0;
