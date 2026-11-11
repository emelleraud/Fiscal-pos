-- =============================================================================
-- 030_dedupe_site_configs_unique.sql
--
-- 018_site_configs_device_type.sql's own dedup+UNIQUE step never actually
-- ran in production (same discrepancy fixed for its column/CHECK in 029).
-- Confirmed via a live read-only query: site 9983f3ac-cde8-4838-9386-
-- 49ef24f57dad has two site_configs rows, identical in every column
-- (same version, same updated_at_ms, same menu content) -- almost
-- certainly the 008 seed's "ON CONFLICT DO NOTHING" silently re-inserting
-- because the UNIQUE constraint it needed to detect a conflict was never
-- there. Safe to drop either duplicate; this keeps the one with the
-- highest updated_at_ms (018's original tie-break), ties broken
-- arbitrarily since the content is identical.
-- =============================================================================

DELETE FROM public.site_configs
  WHERE id NOT IN (
    SELECT DISTINCT ON (site_id, device_type) id
      FROM public.site_configs
      ORDER BY site_id, device_type, updated_at_ms DESC
  );

ALTER TABLE public.site_configs
  ADD CONSTRAINT uq_site_configs_site_device UNIQUE (site_id, device_type);
