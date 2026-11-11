-- =============================================================================
-- 038_revoke_truncate_trigger_references_global.sql
--
-- The same per-project default privilege already found and fixed
-- table-by-table (034: fiscal_entries, 035: z_reports, 036: sessions,
-- 037: sites) was confirmed present on 24 more tables/views (every
-- remaining table in public, plus the two fiscal views): `authenticated`
-- holds TRUNCATE, TRIGGER and REFERENCES nobody ever granted and no
-- policy ever needed. TRUNCATE in particular is never filtered by RLS
-- regardless of role or policy -- demonstrated concretely on four
-- different tables already (a plain 'manager' wiping sites/sessions, a
-- read-only 'pos_auditeur' wiping sessions, etc).
--
-- Rather than keep fixing this one table at a time, revoke it everywhere
-- at once, the same way 032 already did for `anon`'s table grants. This
-- never touches SELECT/INSERT/UPDATE/DELETE, which remain governed by
-- each table's own RLS policies -- that part of this audit stays
-- table-by-table, this is only the three privileges that had zero
-- legitimate use anywhere.
-- =============================================================================

REVOKE TRUNCATE, TRIGGER, REFERENCES ON ALL TABLES IN SCHEMA public FROM authenticated;

ALTER DEFAULT PRIVILEGES IN SCHEMA public
  REVOKE TRUNCATE, TRIGGER, REFERENCES ON TABLES FROM authenticated;
