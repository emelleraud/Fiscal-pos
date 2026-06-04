-- =============================================================================
-- 018_site_configs_device_type.sql
-- Prépare site_configs pour support multi-device (KDS, Kiosk)
-- V1 = 'pos' uniquement. Device_type sera utilisé pour la roadmap KDS/Kiosk.
-- =============================================================================

-- Ajouter la colonne device_type si elle n'existe pas
ALTER TABLE public.site_configs
  ADD COLUMN IF NOT EXISTS device_type text NOT NULL DEFAULT 'pos';

-- Dédupliquer : garder le plus récent (updated_at_ms max) pour chaque (site_id, device_type)
-- Supprimer les doublons
DELETE FROM public.site_configs
  WHERE id NOT IN (
    SELECT DISTINCT ON (site_id, device_type) id
      FROM public.site_configs
      ORDER BY site_id, device_type, updated_at_ms DESC
  );

-- Créer la contrainte UNIQUE(site_id, device_type)
ALTER TABLE public.site_configs
  ADD CONSTRAINT uq_site_configs_site_device UNIQUE (site_id, device_type);
