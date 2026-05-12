-- =============================================================================
-- Migration 006 : Accès lecture anon sur z_reports
--
-- Contexte : le back-office React utilise la clé anon (non authentifiée).
-- La table z_reports n'a aucune policy RLS pour anon → résultat vide.
--
-- Fix : ajouter une policy SELECT pour anon et accorder le GRANT correspondant.
-- La table reste protégée en écriture (INSERT/UPDATE/DELETE) pour les rôles
-- authentifiés uniquement.
-- =============================================================================

-- Policy : anon peut lire tous les rapports Z (back-office mono-site)
create policy "anon_read_z_reports"
    on public.z_reports
    for select
    to anon
    using (true);

-- Grant SELECT au rôle anon
grant select on public.z_reports to anon;
