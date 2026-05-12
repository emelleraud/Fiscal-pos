-- =============================================================================
-- Migration 005 : Lecture anon sur les vues back-office
--
-- Contexte : le back-office React utilise la clé anon Supabase (non authentifiée).
-- Les vues daily_revenue_by_site et fiscal_entries_enriched utilisent
-- SECURITY INVOKER — elles héritent des droits du rôle anon qui n'a aucune
-- policy RLS → résultat vide.
--
-- Fix : passer ces vues en SECURITY DEFINER (exécution avec les droits du
-- owner postgres, qui bypass RLS) et accorder SELECT au rôle anon.
-- Les tables de base restent protégées par RLS pour les accès directs.
-- =============================================================================

-- Vues en SECURITY DEFINER : s'exécutent avec les droits du owner (bypass RLS)
alter view public.fiscal_entries_enriched  set (security_invoker = false);
alter view public.daily_revenue_by_site    set (security_invoker = false);

-- Grants lecture au rôle anon (clé publique du back-office)
grant select on public.fiscal_entries_enriched  to anon;
grant select on public.daily_revenue_by_site     to anon;
