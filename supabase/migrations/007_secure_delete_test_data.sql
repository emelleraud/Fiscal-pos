-- delete_test_data() : usage dev/test uniquement.
-- Restreint l'exécution au service_role ; anon et authenticated n'y ont pas accès.

REVOKE EXECUTE ON FUNCTION public.delete_test_data(uuid) FROM PUBLIC;
REVOKE EXECUTE ON FUNCTION public.delete_test_data(uuid) FROM anon;
REVOKE EXECUTE ON FUNCTION public.delete_test_data(uuid) FROM authenticated;

GRANT EXECUTE ON FUNCTION public.delete_test_data(uuid) TO service_role;

COMMENT ON FUNCTION public.delete_test_data(uuid) IS
  'Supprime les données de test pour un site. Réservé au service_role (dev/CI uniquement).';
