-- =============================================================================
-- 029_site_configs_device_type_check.sql
--
-- Found while adding this CHECK: migration 018_site_configs_device_type.sql
-- is marked applied in schema_migrations, but its actual effect was never
-- present in production -- public.site_configs has no device_type column
-- at all (confirmed via information_schema.columns) and none of its
-- constraints (confirmed via pg_constraint). Re-adding the column here,
-- idempotently, before the CHECK that needs it to exist.
--
-- device_type was free text with no CHECK. Only 'pos' is written anywhere
-- in the codebase today; 018's own comment named 'kds'/'kiosk' as the
-- planned future values -- constraining to those three.
-- =============================================================================

ALTER TABLE public.site_configs
  ADD COLUMN IF NOT EXISTS device_type text NOT NULL DEFAULT 'pos';

ALTER TABLE public.site_configs
  ADD CONSTRAINT chk_site_configs_device_type
  CHECK (device_type IN ('pos', 'kds', 'kiosk'));
