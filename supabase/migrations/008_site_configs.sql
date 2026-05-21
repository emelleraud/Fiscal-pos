-- =============================================================================
-- 008_site_configs.sql
-- Table de configuration par site : menu, prix, version.
-- Lue par sync-client (config_puller) via service_role.
-- =============================================================================

CREATE TABLE IF NOT EXISTS public.site_configs (
    id            uuid    NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,
    site_id       uuid    NOT NULL REFERENCES public.sites(id) ON DELETE CASCADE,
    version       integer NOT NULL DEFAULT 1,
    menu          jsonb   NOT NULL DEFAULT '{}',
    updated_at_ms bigint  NOT NULL DEFAULT (EXTRACT(EPOCH FROM now()) * 1000)::bigint
);

CREATE INDEX IF NOT EXISTS idx_site_configs_site_id
    ON public.site_configs(site_id);

-- RLS activé — seul service_role (sync-client) peut accéder.
-- Les clés anon/authenticated n'ont pas accès à la configuration.
ALTER TABLE public.site_configs ENABLE ROW LEVEL SECURITY;

-- ---------------------------------------------------------------------------
-- Seed : menu de démo pour le site de test
-- ---------------------------------------------------------------------------

INSERT INTO public.site_configs (site_id, version, menu, updated_at_ms)
VALUES (
    '9983f3ac-cde8-4838-9386-49ef24f57dad',
    1,
    '{
      "items": [
        {"id": "burger-001", "name": "Burger Classic",    "price_ttc_cents": 1290, "tva_rate": "intermediaire10", "category": "Burgers",          "available": true},
        {"id": "burger-002", "name": "Double Cheese",     "price_ttc_cents": 1490, "tva_rate": "intermediaire10", "category": "Burgers",          "available": true},
        {"id": "accomp-001", "name": "Frites",            "price_ttc_cents":  390, "tva_rate": "intermediaire10", "category": "Accompagnements",  "available": true},
        {"id": "accomp-002", "name": "Onion Rings",       "price_ttc_cents":  450, "tva_rate": "intermediaire10", "category": "Accompagnements",  "available": true},
        {"id": "boisson-001","name": "Coca-Cola 33cl",    "price_ttc_cents":  320, "tva_rate": "intermediaire10", "category": "Boissons",         "available": true},
        {"id": "boisson-002","name": "Eau minérale 50cl", "price_ttc_cents":  200, "tva_rate": "intermediaire10", "category": "Boissons",         "available": true},
        {"id": "dessert-001","name": "Sundae Vanille",    "price_ttc_cents":  290, "tva_rate": "intermediaire10", "category": "Desserts",         "available": true}
      ]
    }'::jsonb,
    1748476800000
)
ON CONFLICT DO NOTHING;
