-- =============================================================================
-- 031_restrict_catalog_to_director.sql
--
-- menu_categories/menu_products/menu_variants/menu_modifier_groups/
-- menu_modifiers/menu_product_modifier_groups (010) and menu_combos/
-- menu_combo_fixed_items/menu_combo_slots/menu_combo_slot_options (011)
-- are network-wide catalog tables (no site_id by design) with
-- FOR ALL TO authenticated USING (true) WITH CHECK (true) -- any logged-in
-- role, including the lowest-ranked 'manager', could create, edit or
-- delete any product/category/combo/price for the entire chain. The
-- backoffice nav never gated these routes by role either (unlike
-- "Groupes" -> hasRole('director'), or "Réseau" -> pos_admin/regional_director).
--
-- Per decision: reserve the whole catalog (read + write) to director and
-- above (director, regional_director, pos_admin) -- consistent with how
-- "Groupes" is gated in the UI.
-- =============================================================================

DROP POLICY IF EXISTS "backoffice_menu_categories"              ON public.menu_categories;
DROP POLICY IF EXISTS "backoffice_menu_products"                ON public.menu_products;
DROP POLICY IF EXISTS "backoffice_menu_variants"                ON public.menu_variants;
DROP POLICY IF EXISTS "backoffice_menu_modifier_groups"         ON public.menu_modifier_groups;
DROP POLICY IF EXISTS "backoffice_menu_modifiers"               ON public.menu_modifiers;
DROP POLICY IF EXISTS "backoffice_menu_product_modifier_groups" ON public.menu_product_modifier_groups;
DROP POLICY IF EXISTS "backoffice_menu_combos"                  ON public.menu_combos;
DROP POLICY IF EXISTS "backoffice_menu_combo_fixed_items"       ON public.menu_combo_fixed_items;
DROP POLICY IF EXISTS "backoffice_menu_combo_slots"             ON public.menu_combo_slots;
DROP POLICY IF EXISTS "backoffice_menu_combo_slot_options"      ON public.menu_combo_slot_options;

CREATE POLICY "director_up_menu_categories" ON public.menu_categories
  FOR ALL TO authenticated
  USING      (public.auth_app_role() IN ('director', 'regional_director', 'pos_admin'))
  WITH CHECK (public.auth_app_role() IN ('director', 'regional_director', 'pos_admin'));

CREATE POLICY "director_up_menu_products" ON public.menu_products
  FOR ALL TO authenticated
  USING      (public.auth_app_role() IN ('director', 'regional_director', 'pos_admin'))
  WITH CHECK (public.auth_app_role() IN ('director', 'regional_director', 'pos_admin'));

CREATE POLICY "director_up_menu_variants" ON public.menu_variants
  FOR ALL TO authenticated
  USING      (public.auth_app_role() IN ('director', 'regional_director', 'pos_admin'))
  WITH CHECK (public.auth_app_role() IN ('director', 'regional_director', 'pos_admin'));

CREATE POLICY "director_up_menu_modifier_groups" ON public.menu_modifier_groups
  FOR ALL TO authenticated
  USING      (public.auth_app_role() IN ('director', 'regional_director', 'pos_admin'))
  WITH CHECK (public.auth_app_role() IN ('director', 'regional_director', 'pos_admin'));

CREATE POLICY "director_up_menu_modifiers" ON public.menu_modifiers
  FOR ALL TO authenticated
  USING      (public.auth_app_role() IN ('director', 'regional_director', 'pos_admin'))
  WITH CHECK (public.auth_app_role() IN ('director', 'regional_director', 'pos_admin'));

CREATE POLICY "director_up_menu_product_modifier_groups" ON public.menu_product_modifier_groups
  FOR ALL TO authenticated
  USING      (public.auth_app_role() IN ('director', 'regional_director', 'pos_admin'))
  WITH CHECK (public.auth_app_role() IN ('director', 'regional_director', 'pos_admin'));

CREATE POLICY "director_up_menu_combos" ON public.menu_combos
  FOR ALL TO authenticated
  USING      (public.auth_app_role() IN ('director', 'regional_director', 'pos_admin'))
  WITH CHECK (public.auth_app_role() IN ('director', 'regional_director', 'pos_admin'));

CREATE POLICY "director_up_menu_combo_fixed_items" ON public.menu_combo_fixed_items
  FOR ALL TO authenticated
  USING      (public.auth_app_role() IN ('director', 'regional_director', 'pos_admin'))
  WITH CHECK (public.auth_app_role() IN ('director', 'regional_director', 'pos_admin'));

CREATE POLICY "director_up_menu_combo_slots" ON public.menu_combo_slots
  FOR ALL TO authenticated
  USING      (public.auth_app_role() IN ('director', 'regional_director', 'pos_admin'))
  WITH CHECK (public.auth_app_role() IN ('director', 'regional_director', 'pos_admin'));

CREATE POLICY "director_up_menu_combo_slot_options" ON public.menu_combo_slot_options
  FOR ALL TO authenticated
  USING      (public.auth_app_role() IN ('director', 'regional_director', 'pos_admin'))
  WITH CHECK (public.auth_app_role() IN ('director', 'regional_director', 'pos_admin'));
