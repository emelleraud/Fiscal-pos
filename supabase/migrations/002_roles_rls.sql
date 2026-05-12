-- =============================================================================
-- 002_roles_rls.sql
-- Rôles applicatifs et politiques RLS — pos-fiscal / Supabase
-- =============================================================================
-- Rôles :
--   pos_admin    → accès complet à tous les sites (DSI, responsable réseau)
--   pos_caissier → lecture/écriture sur son site uniquement (sync_client Rust)
--   pos_auditeur → lecture seule sur tous les sites (contrôleur, expert-comptable)
--
-- Convention JWT :
--   Le claim `app_metadata.site_id` porte l'UUID du site pour pos_caissier.
--   Le claim `app_metadata.role`    porte le rôle applicatif.
-- =============================================================================

-- ---------------------------------------------------------------------------
-- Rôles PostgreSQL (au niveau base de données)
-- ---------------------------------------------------------------------------
do $$ begin
    if not exists (select 1 from pg_roles where rolname = 'pos_admin') then
        create role pos_admin;
    end if;
    if not exists (select 1 from pg_roles where rolname = 'pos_caissier') then
        create role pos_caissier;
    end if;
    if not exists (select 1 from pg_roles where rolname = 'pos_auditeur') then
        create role pos_auditeur;
    end if;
end $$;

-- ---------------------------------------------------------------------------
-- Fonction helper : extraire le site_id depuis le JWT Supabase
-- Utilisée dans les policies RLS pour pos_caissier.
-- ---------------------------------------------------------------------------
create or replace function public.auth_site_id()
returns uuid
language sql stable
as $$
    select nullif(
        (auth.jwt() -> 'app_metadata' ->> 'site_id'),
        ''
    )::uuid;
$$;

-- ---------------------------------------------------------------------------
-- Fonction helper : extraire le rôle applicatif depuis le JWT
-- ---------------------------------------------------------------------------
create or replace function public.auth_app_role()
returns text
language sql stable
as $$
    select auth.jwt() -> 'app_metadata' ->> 'role';
$$;

-- =============================================================================
-- TABLE : sites
-- =============================================================================

-- pos_admin : tout voir, tout modifier
create policy "admin_all_sites"
    on public.sites
    for all
    to authenticated
    using (public.auth_app_role() = 'pos_admin')
    with check (public.auth_app_role() = 'pos_admin');

-- pos_caissier : voir uniquement son site
create policy "caissier_own_site"
    on public.sites
    for select
    to authenticated
    using (id = public.auth_site_id());

-- pos_auditeur : voir tous les sites en lecture seule
create policy "auditeur_read_sites"
    on public.sites
    for select
    to authenticated
    using (public.auth_app_role() = 'pos_auditeur');

-- =============================================================================
-- TABLE : sessions
-- =============================================================================

-- pos_admin : accès complet
create policy "admin_all_sessions"
    on public.sessions
    for all
    to authenticated
    using (public.auth_app_role() = 'pos_admin')
    with check (public.auth_app_role() = 'pos_admin');

-- pos_caissier : CRUD sur les sessions de son site
create policy "caissier_own_sessions"
    on public.sessions
    for all
    to authenticated
    using (site_id = public.auth_site_id())
    with check (site_id = public.auth_site_id());

-- pos_auditeur : lecture seule tous sites
create policy "auditeur_read_sessions"
    on public.sessions
    for select
    to authenticated
    using (public.auth_app_role() = 'pos_auditeur');

-- =============================================================================
-- TABLE : fiscal_entries
-- =============================================================================

-- pos_admin : lecture + insertion (pas de update/delete — règles immuables)
create policy "admin_read_fiscal_entries"
    on public.fiscal_entries
    for select
    to authenticated
    using (public.auth_app_role() = 'pos_admin');

create policy "admin_insert_fiscal_entries"
    on public.fiscal_entries
    for insert
    to authenticated
    with check (public.auth_app_role() = 'pos_admin');

-- pos_caissier : insert + select sur son site uniquement
-- (le sync_client Rust pousse les entrées ; pas de modification possible)
create policy "caissier_insert_fiscal_entries"
    on public.fiscal_entries
    for insert
    to authenticated
    with check (site_id = public.auth_site_id());

create policy "caissier_read_own_fiscal_entries"
    on public.fiscal_entries
    for select
    to authenticated
    using (site_id = public.auth_site_id());

-- pos_auditeur : lecture seule tous sites
create policy "auditeur_read_fiscal_entries"
    on public.fiscal_entries
    for select
    to authenticated
    using (public.auth_app_role() = 'pos_auditeur');

-- =============================================================================
-- TABLE : z_reports
-- =============================================================================

-- pos_admin : accès complet
create policy "admin_all_z_reports"
    on public.z_reports
    for all
    to authenticated
    using (public.auth_app_role() = 'pos_admin')
    with check (public.auth_app_role() = 'pos_admin');

-- pos_caissier : insert + select sur son site
create policy "caissier_insert_z_reports"
    on public.z_reports
    for insert
    to authenticated
    with check (site_id = public.auth_site_id());

create policy "caissier_read_own_z_reports"
    on public.z_reports
    for select
    to authenticated
    using (site_id = public.auth_site_id());

-- pos_auditeur : lecture seule tous sites
create policy "auditeur_read_z_reports"
    on public.z_reports
    for select
    to authenticated
    using (public.auth_app_role() = 'pos_auditeur');

-- =============================================================================
-- Grants sur les vues (lecture seule — pas de RLS sur les vues)
-- Les vues héritent de la sécurité des tables sous-jacentes via SECURITY INVOKER
-- =============================================================================
grant select on public.fiscal_entries_enriched  to authenticated;
grant select on public.daily_revenue_by_site     to authenticated;

-- Assurer que les vues s'exécutent avec les droits de l'appelant (pas du owner)
alter view public.fiscal_entries_enriched  set (security_invoker = true);
alter view public.daily_revenue_by_site    set (security_invoker = true);

-- =============================================================================
-- Grants tables → rôles PostgreSQL
-- =============================================================================

-- pos_admin
grant select, insert, update, delete on public.sites           to pos_admin;
grant select, insert, update, delete on public.sessions        to pos_admin;
grant select, insert                 on public.fiscal_entries  to pos_admin;
grant select, insert, update, delete on public.z_reports       to pos_admin;

-- pos_caissier
grant select                         on public.sites           to pos_caissier;
grant select, insert, update         on public.sessions        to pos_caissier;
grant select, insert                 on public.fiscal_entries  to pos_caissier;
grant select, insert                 on public.z_reports       to pos_caissier;

-- pos_auditeur
grant select on public.sites           to pos_auditeur;
grant select on public.sessions        to pos_auditeur;
grant select on public.fiscal_entries  to pos_auditeur;
grant select on public.z_reports       to pos_auditeur;

-- =============================================================================
-- Service role (sync_client Rust en mode service key) : bypass RLS
-- Le sync_client utilise la service_role key → pas soumis aux policies RLS.
-- Documenté ici pour traçabilité ; aucune policy n'est nécessaire.
-- =============================================================================
-- Note : en production, le sync_client Rust doit utiliser un JWT signé avec
-- app_metadata.role = 'pos_caissier' et app_metadata.site_id = <uuid_du_site>
-- pour que les policies RLS s'appliquent correctement.
-- La service_role key ne doit être utilisée que pour les migrations et l'admin.
-- =============================================================================
