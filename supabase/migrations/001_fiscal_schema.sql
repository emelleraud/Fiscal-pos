-- =============================================================================
-- 001_fiscal_schema.sql
-- Schéma fiscal NF525 — pos-fiscal / Supabase
-- =============================================================================
-- Ordre de création : extensions → sites → sessions → fiscal_entries → z_reports
-- RLS activé sur toutes les tables métier (policies dans 002_roles_rls.sql)
-- =============================================================================

-- ---------------------------------------------------------------------------
-- Extensions
-- ---------------------------------------------------------------------------
-- uuid-ossp non requis sur Supabase : gen_random_uuid() est natif (PG 13+)
create extension if not exists "pgcrypto";

-- ---------------------------------------------------------------------------
-- Table : sites
-- Un site = un établissement (restaurant). Clé d isolation RLS.
-- ---------------------------------------------------------------------------
create table if not exists public.sites (
    id            uuid        primary key default gen_random_uuid(),
    site_code     text        not null unique,          -- ex: "QSR-75001"
    name          text        not null,
    address       text,
    siret         char(14),
    created_at    timestamptz not null default now(),
    updated_at    timestamptz not null default now()
);

comment on table public.sites is
    'Référentiel des établissements. Clé d isolation RLS pour toutes les tables fiscales.';

-- ---------------------------------------------------------------------------
-- Table : sessions
-- Session de caisse (ouverture → clôture). Correspond à une "Session" Rust.
-- ---------------------------------------------------------------------------
create table if not exists public.sessions (
    id                uuid        primary key default gen_random_uuid(),
    site_id           uuid        not null references public.sites(id) on delete restrict,
    session_ref       text        not null,             -- identifiant métier opaque (ex: UUID côté POS)
    opened_at         timestamptz not null,
    closed_at         timestamptz,                      -- null = session encore ouverte
    operator_id       text,                             -- identifiant caissier (opaque)
    opening_amount    bigint      not null default 0,   -- Cents
    status            text        not null default 'open'
                          check (status in ('open', 'closed', 'error')),
    created_at        timestamptz not null default now(),

    unique (site_id, session_ref)
);

comment on table public.sessions is
    'Sessions de caisse. Une session est ouverte par un opérateur sur un site.';
comment on column public.sessions.opening_amount is
    'Fonds de caisse a l ouverture, en centimes.';

create index idx_sessions_site_id    on public.sessions(site_id);
create index idx_sessions_status     on public.sessions(status);
create index idx_sessions_opened_at  on public.sessions(opened_at);

-- ---------------------------------------------------------------------------
-- Table : fiscal_entries
-- Cœur du journal fiscal. Immutable après insertion (pas d'UPDATE/DELETE).
-- Correspond exactement au payload produit par le serializer Rust.
-- ---------------------------------------------------------------------------
create table if not exists public.fiscal_entries (
    id              uuid        primary key default gen_random_uuid(),
    site_id         uuid        not null references public.sites(id) on delete restrict,
    session_id      uuid        references public.sessions(id) on delete restrict,

    -- Champs identiques au FiscalEntryPayload du sync_client Rust
    sequence        bigint      not null,               -- numéro séquentiel croissant par site
    operation_type  text        not null
                        check (operation_type in (
                            'SALE', 'REFUND', 'DISCOUNT',
                            'VOID', 'TRAINING', 'CORRECTION'
                        )),
    amount          bigint      not null,               -- Cents, signé (négatif = remboursement)
    tva_rate        text        not null
                        check (tva_rate in ('5.5', '10', '20')),
    order_ref       text,                               -- référence commande POS
    entry_hash      char(64)    not null,               -- SHA-256 hex (chaîne fiscale)
    prev_hash       char(64),                           -- hash de l'entrée précédente (null si première)
    signed_at       timestamptz not null,               -- timestamp côté POS

    -- Métadonnées de sync
    synced_at       timestamptz not null default now(), -- timestamp d'insertion Supabase
    sync_batch_id   uuid,                               -- identifiant du batch d'envoi

    -- Contrainte d'unicité : (site_id, sequence) garantit l'ordre du journal
    unique (site_id, sequence)
);

comment on table public.fiscal_entries is
    'Journal fiscal immuable. Chaque ligne correspond à un FiscalEntry signé par le microservice Rust.';
comment on column public.fiscal_entries.sequence is
    'Numéro séquentiel par site. Strictement croissant, sans gap. Garanti par le fiscal-engine.';
comment on column public.fiscal_entries.entry_hash is
    'SHA-256 de l entree courante (chaîne de hachage NF525).';
comment on column public.fiscal_entries.prev_hash is
    'SHA-256 de l entree precedente. NULL uniquement pour la première entrée du site.';

-- Index de performance et d'audit
create index idx_fiscal_entries_site_id     on public.fiscal_entries(site_id);
create index idx_fiscal_entries_session_id  on public.fiscal_entries(session_id);
create index idx_fiscal_entries_signed_at   on public.fiscal_entries(signed_at);
create index idx_fiscal_entries_sequence    on public.fiscal_entries(site_id, sequence);
create index idx_fiscal_entries_op_type     on public.fiscal_entries(operation_type);

-- Interdire UPDATE et DELETE (journal immuable)
create or replace rule fiscal_entries_no_update as
    on update to public.fiscal_entries do instead nothing;

create or replace rule fiscal_entries_no_delete as
    on delete to public.fiscal_entries do instead nothing;

-- ---------------------------------------------------------------------------
-- Table : z_reports
-- Rapport de clôture journalière (rapport Z). Un par session fermée.
-- ---------------------------------------------------------------------------
create table if not exists public.z_reports (
    id                  uuid        primary key default gen_random_uuid(),
    site_id             uuid        not null references public.sites(id) on delete restrict,
    session_id          uuid        not null unique references public.sessions(id) on delete restrict,

    report_number       bigint      not null,           -- numéro séquentiel de rapport Z par site
    generated_at        timestamptz not null,

    -- Totaux agrégés (Cents)
    total_sales         bigint      not null default 0,
    total_refunds       bigint      not null default 0,
    total_discounts     bigint      not null default 0,
    net_revenue         bigint      not null default 0, -- total_sales + total_refunds + total_discounts

    -- Ventilation TVA (Cents)
    tva_5_5_base_ht     bigint      not null default 0,
    tva_5_5_amount      bigint      not null default 0,
    tva_10_base_ht      bigint      not null default 0,
    tva_10_amount       bigint      not null default 0,
    tva_20_base_ht      bigint      not null default 0,
    tva_20_amount       bigint      not null default 0,

    -- Intégrité
    entry_count         integer     not null default 0,
    first_sequence      bigint,
    last_sequence       bigint,
    report_hash         char(64),                       -- hash du rapport signé

    -- Texte brut (sortie format_z_report_text du Rust)
    report_text         text,

    created_at          timestamptz not null default now(),

    unique (site_id, report_number)
);

comment on table public.z_reports is
    'Rapports Z de clôture journalière. Généré par le z_report_engine Rust, stocké ici pour audit.';
comment on column public.z_reports.net_revenue is
    'Chiffre d affaires net en centimes : total_sales + total_refunds + total_discounts.';

create index idx_z_reports_site_id      on public.z_reports(site_id);
create index idx_z_reports_generated_at on public.z_reports(generated_at);

-- ---------------------------------------------------------------------------
-- Trigger : updated_at automatique sur sites
-- ---------------------------------------------------------------------------
create or replace function public.set_updated_at()
returns trigger language plpgsql as $$
begin
    new.updated_at = now();
    return new;
end;
$$;

create trigger sites_updated_at
    before update on public.sites
    for each row execute function public.set_updated_at();

-- ---------------------------------------------------------------------------
-- Vue : fiscal_entries_with_site
-- Jointure pratique pour le back-office React (évite les joins côté client)
-- ---------------------------------------------------------------------------
create or replace view public.fiscal_entries_enriched as
    select
        fe.*,
        s.site_code,
        s.name          as site_name,
        sess.session_ref,
        sess.operator_id
    from public.fiscal_entries fe
    join public.sites           s    on s.id    = fe.site_id
    left join public.sessions   sess on sess.id = fe.session_id;

comment on view public.fiscal_entries_enriched is
    'Vue dénormalisée pour le back-office : fiscal_entries + site_code + session_ref.';

-- ---------------------------------------------------------------------------
-- Vue : daily_revenue_by_site
-- Agrégat journalier pour le dashboard back-office
-- ---------------------------------------------------------------------------
create or replace view public.daily_revenue_by_site as
    select
        site_id,
        s.site_code,
        s.name                                         as site_name,
        date_trunc('day', fe.signed_at)::date          as day,
        count(*) filter (where operation_type = 'SALE')    as sale_count,
        count(*) filter (where operation_type = 'REFUND')  as refund_count,
        coalesce(sum(amount) filter (where operation_type = 'SALE'),     0) as total_sales,
        coalesce(sum(amount) filter (where operation_type = 'REFUND'),   0) as total_refunds,
        coalesce(sum(amount), 0)                           as net_revenue
    from public.fiscal_entries fe
    join public.sites s on s.id = fe.site_id
    group by fe.site_id, s.site_code, s.name, date_trunc('day', fe.signed_at)::date;

comment on view public.daily_revenue_by_site is
    'Agrégat CA journalier par site pour le dashboard back-office.';

-- ---------------------------------------------------------------------------
-- Activation RLS (policies définies dans 002_roles_rls.sql)
-- ---------------------------------------------------------------------------
alter table public.sites           enable row level security;
alter table public.sessions        enable row level security;
alter table public.fiscal_entries  enable row level security;
alter table public.z_reports       enable row level security;
