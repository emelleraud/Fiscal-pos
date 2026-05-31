-- supabase/migrations/015_promotions_policy_fixes.sql
-- Fix manager UPDATE policy gap + INSERT site enforcement + GRANTs + indexes + trigger

-- Fix 1: Drop and recreate manager INSERT policy with site_id enforcement
DROP POLICY IF EXISTS "manager_write_site_promotions" ON public.promotions;
CREATE POLICY "manager_write_site_promotions" ON public.promotions
  FOR INSERT TO authenticated
  WITH CHECK (
    public.auth_app_role() IN ('manager','director','regional_director')
    AND scope = 'site'
    AND (
      public.auth_app_role() IN ('director','regional_director')
      OR site_id = public.auth_site_id()
    )
  );

-- Fix 2: Add manager UPDATE policy
CREATE POLICY "manager_update_own_site_promotions" ON public.promotions
  FOR UPDATE TO authenticated
  USING (
    public.auth_app_role() = 'manager'
    AND scope = 'site'
    AND site_id = public.auth_site_id()
  )
  WITH CHECK (
    public.auth_app_role() = 'manager'
    AND scope = 'site'
    AND site_id = public.auth_site_id()
  );

-- Fix 3: GRANTs
GRANT SELECT, INSERT, UPDATE, DELETE ON public.restaurant_groups TO authenticated;
GRANT SELECT, INSERT, UPDATE, DELETE ON public.restaurant_group_members TO authenticated;
GRANT SELECT, INSERT, UPDATE, DELETE ON public.promotion_approval_thresholds TO authenticated;
GRANT SELECT, INSERT, UPDATE, DELETE ON public.promotions TO authenticated;

-- Fix 4: Indexes
CREATE INDEX IF NOT EXISTS idx_promotions_site_id      ON public.promotions(site_id) WHERE site_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_promotions_group_id     ON public.promotions(group_id) WHERE group_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_promotions_status       ON public.promotions(status);
CREATE INDEX IF NOT EXISTS idx_promotions_active_scope ON public.promotions(active, scope);
CREATE INDEX IF NOT EXISTS idx_promotions_updated_ms   ON public.promotions(updated_at_ms);
CREATE INDEX IF NOT EXISTS idx_rgm_site_id             ON public.restaurant_group_members(site_id);

-- Fix 5: updated_at_ms trigger
CREATE OR REPLACE FUNCTION public.set_updated_at_ms()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
  NEW.updated_at_ms := (extract(epoch from now()) * 1000)::bigint;
  RETURN NEW;
END;
$$;

CREATE TRIGGER promotions_updated_at_ms
  BEFORE UPDATE ON public.promotions
  FOR EACH ROW EXECUTE FUNCTION public.set_updated_at_ms();
