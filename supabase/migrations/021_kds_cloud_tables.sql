-- =============================================================================
-- 021_kds_cloud_tables.sql
-- Tables cloud KDS : config stations, profils, règles, déclencheurs, seuils.
-- Écrites depuis le backoffice (pos_admin / regional_director).
-- Lues par sync-client (service_role) pour pull → SQLite local.
-- =============================================================================

-- ---------------------------------------------------------------------------
-- kds_routing_profiles  (profils de routage par site)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS public.kds_routing_profiles (
    id      TEXT NOT NULL,
    site_id UUID NOT NULL REFERENCES public.sites(id) ON DELETE CASCADE,
    name    TEXT NOT NULL,
    description TEXT,
    PRIMARY KEY (site_id, id)
);

CREATE INDEX IF NOT EXISTS idx_kds_routing_profiles_site
    ON public.kds_routing_profiles(site_id);

ALTER TABLE public.kds_routing_profiles ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "kds_rp_read" ON public.kds_routing_profiles;
CREATE POLICY "kds_rp_read" ON public.kds_routing_profiles
    FOR SELECT TO authenticated USING (public.can_access_site(site_id));

DROP POLICY IF EXISTS "kds_rp_write" ON public.kds_routing_profiles;
CREATE POLICY "kds_rp_write" ON public.kds_routing_profiles
    FOR ALL TO authenticated
    USING    (public.auth_app_role() IN ('pos_admin','regional_director'))
    WITH CHECK (public.auth_app_role() IN ('pos_admin','regional_director'));

DROP POLICY IF EXISTS "kds_rp_service" ON public.kds_routing_profiles;
CREATE POLICY "kds_rp_service" ON public.kds_routing_profiles
    FOR ALL TO service_role USING (true) WITH CHECK (true);

GRANT SELECT, INSERT, UPDATE, DELETE ON public.kds_routing_profiles TO authenticated;

-- Seed profils normaux pour tous les sites existants
INSERT INTO public.kds_routing_profiles (id, site_id, name, description)
SELECT 'normal', id, 'Service normal', 'Stations polyvalentes'
FROM public.sites
ON CONFLICT DO NOTHING;

INSERT INTO public.kds_routing_profiles (id, site_id, name, description)
SELECT 'rush', id, 'Rush', 'Stations spécialisées, flux élevé'
FROM public.sites
ON CONFLICT DO NOTHING;

-- ---------------------------------------------------------------------------
-- kds_station_configs  (stations par site)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS public.kds_station_configs (
    id                  TEXT    NOT NULL,
    site_id             UUID    NOT NULL REFERENCES public.sites(id) ON DELETE CASCADE,
    name                TEXT    NOT NULL,
    role                TEXT    NOT NULL CHECK (role IN ('preparation','holding','assembly','ready_board')),
    temperature_group   TEXT    CHECK (temperature_group IN ('hot','cold','other')),
    output_type         TEXT    NOT NULL CHECK (output_type IN ('screen','printer','both')),
    printer_address     TEXT,
    printer_type        TEXT    CHECK (printer_type IN ('tcpip','usb','file')),
    printer_mode        TEXT    CHECK (printer_mode IN ('receipt','linerless_label')),
    paper_width_mm      INTEGER CHECK (paper_width_mm IN (50, 80)),
    fallback_station_id TEXT,
    active_in_profiles  TEXT    NOT NULL DEFAULT '["normal"]',
    sort_order          INTEGER NOT NULL DEFAULT 0,
    enabled             INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (site_id, id)
);

CREATE INDEX IF NOT EXISTS idx_kds_station_configs_site
    ON public.kds_station_configs(site_id);

ALTER TABLE public.kds_station_configs ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "kds_sc_read" ON public.kds_station_configs;
CREATE POLICY "kds_sc_read" ON public.kds_station_configs
    FOR SELECT TO authenticated USING (public.can_access_site(site_id));

DROP POLICY IF EXISTS "kds_sc_write" ON public.kds_station_configs;
CREATE POLICY "kds_sc_write" ON public.kds_station_configs
    FOR ALL TO authenticated
    USING    (public.auth_app_role() IN ('pos_admin','regional_director'))
    WITH CHECK (public.auth_app_role() IN ('pos_admin','regional_director'));

DROP POLICY IF EXISTS "kds_sc_service" ON public.kds_station_configs;
CREATE POLICY "kds_sc_service" ON public.kds_station_configs
    FOR ALL TO service_role USING (true) WITH CHECK (true);

GRANT SELECT, INSERT, UPDATE, DELETE ON public.kds_station_configs TO authenticated;

-- ---------------------------------------------------------------------------
-- kds_routing_configs  (règles de routage par site + profil)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS public.kds_routing_configs (
    id          TEXT    NOT NULL,
    site_id     UUID    NOT NULL REFERENCES public.sites(id) ON DELETE CASCADE,
    profile_id  TEXT    NOT NULL,
    rule_type   TEXT    NOT NULL CHECK (rule_type IN ('category','product','tag')),
    match_value TEXT    NOT NULL,
    station_ids TEXT    NOT NULL,
    priority    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (site_id, id)
);

CREATE INDEX IF NOT EXISTS idx_kds_routing_configs_site
    ON public.kds_routing_configs(site_id);

ALTER TABLE public.kds_routing_configs ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "kds_rc_read" ON public.kds_routing_configs;
CREATE POLICY "kds_rc_read" ON public.kds_routing_configs
    FOR SELECT TO authenticated USING (public.can_access_site(site_id));

DROP POLICY IF EXISTS "kds_rc_write" ON public.kds_routing_configs;
CREATE POLICY "kds_rc_write" ON public.kds_routing_configs
    FOR ALL TO authenticated
    USING    (public.auth_app_role() IN ('pos_admin','regional_director'))
    WITH CHECK (public.auth_app_role() IN ('pos_admin','regional_director'));

DROP POLICY IF EXISTS "kds_rc_service" ON public.kds_routing_configs;
CREATE POLICY "kds_rc_service" ON public.kds_routing_configs
    FOR ALL TO service_role USING (true) WITH CHECK (true);

GRANT SELECT, INSERT, UPDATE, DELETE ON public.kds_routing_configs TO authenticated;

-- ---------------------------------------------------------------------------
-- kds_channel_triggers  (déclencheurs KDS par canal × order_type × site)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS public.kds_channel_triggers (
    site_id    UUID NOT NULL REFERENCES public.sites(id) ON DELETE CASCADE,
    channel    TEXT NOT NULL,
    order_type TEXT NOT NULL,
    trigger_on TEXT NOT NULL CHECK (trigger_on IN ('order','payment','both')),
    orb_type   TEXT CHECK (orb_type IN ('client','livreur')),
    PRIMARY KEY (site_id, channel, order_type)
);

CREATE INDEX IF NOT EXISTS idx_kds_channel_triggers_site
    ON public.kds_channel_triggers(site_id);

ALTER TABLE public.kds_channel_triggers ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "kds_ct_read" ON public.kds_channel_triggers;
CREATE POLICY "kds_ct_read" ON public.kds_channel_triggers
    FOR SELECT TO authenticated USING (public.can_access_site(site_id));

DROP POLICY IF EXISTS "kds_ct_write" ON public.kds_channel_triggers;
CREATE POLICY "kds_ct_write" ON public.kds_channel_triggers
    FOR ALL TO authenticated
    USING    (public.auth_app_role() IN ('pos_admin','regional_director'))
    WITH CHECK (public.auth_app_role() IN ('pos_admin','regional_director'));

DROP POLICY IF EXISTS "kds_ct_service" ON public.kds_channel_triggers;
CREATE POLICY "kds_ct_service" ON public.kds_channel_triggers
    FOR ALL TO service_role USING (true) WITH CHECK (true);

GRANT SELECT, INSERT, UPDATE, DELETE ON public.kds_channel_triggers TO authenticated;

-- Seed déclencheurs par défaut pour les sites existants
INSERT INTO public.kds_channel_triggers (site_id, channel, order_type, trigger_on, orb_type)
SELECT s.id, c.channel, c.order_type, c.trigger_on, c.orb_type
FROM public.sites s
CROSS JOIN (VALUES
    ('caisse',   'eat_in',          'payment', NULL),
    ('caisse',   'takeaway',        'payment', 'client'),
    ('kiosk',    'eat_in',          'order',   NULL),
    ('kiosk',    'takeaway',        'order',   'client'),
    ('drive',    'drive',           'payment', NULL),
    ('delivery', 'delivery',        'order',   'livreur'),
    ('delivery', 'click_and_collect','order',  'client')
) AS c(channel, order_type, trigger_on, orb_type)
ON CONFLICT DO NOTHING;

-- ---------------------------------------------------------------------------
-- kds_timer_thresholds  (seuils timer par station × site)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS public.kds_timer_thresholds (
    site_id       UUID    NOT NULL REFERENCES public.sites(id) ON DELETE CASCADE,
    station_id    TEXT    NOT NULL,
    warning_secs  INTEGER NOT NULL DEFAULT 120,
    critical_secs INTEGER NOT NULL DEFAULT 300,
    PRIMARY KEY (site_id, station_id)
);

CREATE INDEX IF NOT EXISTS idx_kds_timer_thresholds_site
    ON public.kds_timer_thresholds(site_id);

ALTER TABLE public.kds_timer_thresholds ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "kds_tt_read" ON public.kds_timer_thresholds;
CREATE POLICY "kds_tt_read" ON public.kds_timer_thresholds
    FOR SELECT TO authenticated USING (public.can_access_site(site_id));

DROP POLICY IF EXISTS "kds_tt_write" ON public.kds_timer_thresholds;
CREATE POLICY "kds_tt_write" ON public.kds_timer_thresholds
    FOR ALL TO authenticated
    USING    (public.auth_app_role() IN ('pos_admin','regional_director'))
    WITH CHECK (public.auth_app_role() IN ('pos_admin','regional_director'));

DROP POLICY IF EXISTS "kds_tt_service" ON public.kds_timer_thresholds;
CREATE POLICY "kds_tt_service" ON public.kds_timer_thresholds
    FOR ALL TO service_role USING (true) WITH CHECK (true);

GRANT SELECT, INSERT, UPDATE, DELETE ON public.kds_timer_thresholds TO authenticated;
