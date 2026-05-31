-- supabase/migrations/013_promotion_approval_thresholds.sql
CREATE TABLE IF NOT EXISTS public.promotion_approval_thresholds (
  id            uuid  PRIMARY KEY DEFAULT gen_random_uuid(),
  scope         text  NOT NULL CHECK (scope IN ('site','group','chain')),
  max_cents     int,          -- NULL = illimité
  required_role text  NOT NULL CHECK (required_role IN ('manager','director','regional_director'))
);

ALTER TABLE public.promotion_approval_thresholds ENABLE ROW LEVEL SECURITY;

CREATE POLICY "service_role_all_thresholds" ON public.promotion_approval_thresholds
  FOR ALL TO service_role USING (true) WITH CHECK (true);
CREATE POLICY "authenticated_read_thresholds" ON public.promotion_approval_thresholds
  FOR SELECT TO authenticated USING (true);
CREATE POLICY "regional_director_write_thresholds" ON public.promotion_approval_thresholds
  FOR ALL TO authenticated
  USING ((auth.jwt() -> 'app_metadata' ->> 'role') = 'regional_director')
  WITH CHECK ((auth.jwt() -> 'app_metadata' ->> 'role') = 'regional_director');

-- Seed des seuils par défaut
INSERT INTO public.promotion_approval_thresholds (scope, max_cents, required_role) VALUES
  ('site',  1000, 'manager'),
  ('site',  NULL, 'director'),
  ('group', NULL, 'director'),
  ('chain', NULL, 'regional_director');

GRANT SELECT, INSERT, UPDATE, DELETE ON public.promotion_approval_thresholds TO authenticated;
