-- =============================================================================
-- 004_fix_immutability_rules.sql
-- Remplace les RULE NO UPDATE/DELETE par des triggers BEFORE
-- Les RULE sont incompatibles avec INSERT ... ON CONFLICT (PGRST/PostgREST)
-- =============================================================================

-- Supprimer les anciennes règles
drop rule if exists fiscal_entries_no_update on public.fiscal_entries;
drop rule if exists fiscal_entries_no_delete on public.fiscal_entries;

-- Trigger BEFORE UPDATE : bloque toute modification
create or replace function public.fiscal_entries_prevent_update()
returns trigger language plpgsql as $$
begin
    raise exception 'fiscal_entries est immuable : les mises à jour sont interdites (NF525)';
end;
$$;

create trigger fiscal_entries_no_update
    before update on public.fiscal_entries
    for each row execute function public.fiscal_entries_prevent_update();

-- Trigger BEFORE DELETE : bloque toute suppression
create or replace function public.fiscal_entries_prevent_delete()
returns trigger language plpgsql as $$
begin
    raise exception 'fiscal_entries est immuable : les suppressions sont interdites (NF525)';
end;
$$;

create trigger fiscal_entries_no_delete
    before delete on public.fiscal_entries
    for each row execute function public.fiscal_entries_prevent_delete();
