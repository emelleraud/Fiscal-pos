-- =============================================================================
-- supabase/seed.sql
-- Données de seed pour le développement local
-- NE PAS appliquer en production.
-- =============================================================================
-- Usage : supabase db reset  (recrée la DB locale + applique seed.sql)
-- =============================================================================

-- ---------------------------------------------------------------------------
-- Sites de test
-- ---------------------------------------------------------------------------
insert into public.sites (id, site_code, name, address, siret)
values
    ('11111111-1111-1111-1111-111111111111', 'QSR-75001', 'QSR Paris Centre',     '42 rue de Rivoli, 75001 Paris',        '12345678900001'),
    ('22222222-2222-2222-2222-222222222222', 'QSR-94000', 'QSR Créteil Val-de-Marne', '5 allée des Roses, 94000 Créteil', '12345678900002'),
    ('33333333-3333-3333-3333-333333333333', 'QSR-77000', 'QSR Melun Seine-et-Marne', '10 place Saint-Jean, 77000 Melun', '12345678900003')
on conflict (site_code) do nothing;

-- ---------------------------------------------------------------------------
-- Session de test (site QSR-75001)
-- ---------------------------------------------------------------------------
insert into public.sessions (id, site_id, session_ref, opened_at, operator_id, opening_amount, status)
values
    (
        'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
        '11111111-1111-1111-1111-111111111111',
        'sess-2025-001',
        '2025-01-15 08:00:00+01',
        'operateur-01',
        10000,   -- 100,00 € en centimes
        'closed'
    )
on conflict (site_id, session_ref) do nothing;

-- ---------------------------------------------------------------------------
-- Entrées fiscales de test (chaîne de 5 entrées)
-- Hashes fictifs pour le seed — le fiscal-engine génère les vrais hashes.
-- ---------------------------------------------------------------------------
insert into public.fiscal_entries
    (site_id, session_id, sequence, operation_type, amount, tva_rate, order_ref, entry_hash, prev_hash, signed_at)
values
    (
        '11111111-1111-1111-1111-111111111111',
        'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
        1, 'SALE', 1299, '10',
        'ORD-000001',
        'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
        null,
        '2025-01-15 08:12:00+01'
    ),
    (
        '11111111-1111-1111-1111-111111111111',
        'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
        2, 'SALE', 850, '5.5',
        'ORD-000002',
        'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
        'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
        '2025-01-15 08:35:00+01'
    ),
    (
        '11111111-1111-1111-1111-111111111111',
        'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
        3, 'SALE', 2490, '20',
        'ORD-000003',
        'cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc',
        'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
        '2025-01-15 09:10:00+01'
    ),
    (
        '11111111-1111-1111-1111-111111111111',
        'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
        4, 'REFUND', -850, '5.5',
        'ORD-000002',
        'dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd',
        'cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc',
        '2025-01-15 09:45:00+01'
    ),
    (
        '11111111-1111-1111-1111-111111111111',
        'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
        5, 'SALE', 590, '10',
        'ORD-000004',
        'eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee',
        'dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd',
        '2025-01-15 10:20:00+01'
    )
on conflict (site_id, sequence) do nothing;

-- ---------------------------------------------------------------------------
-- Rapport Z de test
-- ---------------------------------------------------------------------------
insert into public.z_reports (
    site_id, session_id, report_number, generated_at,
    total_sales, total_refunds, total_discounts, net_revenue,
    tva_5_5_base_ht, tva_5_5_amount,
    tva_10_base_ht,  tva_10_amount,
    tva_20_base_ht,  tva_20_amount,
    entry_count, first_sequence, last_sequence,
    report_text
)
values (
    '11111111-1111-1111-1111-111111111111',
    'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
    1,
    '2025-01-15 23:59:00+01',
    -- total_sales : 1299 + 850 + 2490 + 590 = 5229
    5229,
    -- total_refunds : -850
    -850,
    0,
    -- net_revenue : 5229 - 850 = 4379
    4379,
    -- TVA 5.5% : base 850 → HT = 806, TVA = 44
    806, 44,
    -- TVA 10% : base 1299+590=1889 → HT = 1717, TVA = 172
    1717, 172,
    -- TVA 20% : base 2490 → HT = 2075, TVA = 415
    2075, 415,
    5, 1, 5,
    '=== RAPPORT Z #1 ===
Site    : QSR Paris Centre
Date    : 15/01/2025
CA net  : 43,79 €
Entrées : 5'
)
on conflict (site_id, report_number) do nothing;
