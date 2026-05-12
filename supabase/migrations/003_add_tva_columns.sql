-- =============================================================================
-- 003_add_tva_columns.sql
-- Ajout des colonnes ht_cents et tva_cents à fiscal_entries
-- Alignement avec le FiscalEntryPayload du sync-client Rust
-- =============================================================================

alter table public.fiscal_entries
    add column if not exists ht_cents   bigint,
    add column if not exists tva_cents  bigint;

comment on column public.fiscal_entries.ht_cents  is
    'Montant HT en centimes (signé). Calculé par le fiscal-engine Rust.';
comment on column public.fiscal_entries.tva_cents is
    'Montant TVA en centimes (signé). Calculé par le fiscal-engine Rust.';
