INSERT INTO merchants (
    id, name, email, api_key, webhook_url,
    status, kyc_status, risk_level, created_at, updated_at
) VALUES (
    '00000000-0000-0000-0000-000000000001',
    'Demo Merchant',
    'demo@paybank.com',
    'pb_test_1234567890abcdef',
    'http://localhost:4004/webhooks',
    'active', 'verified', 'low',
    NOW(), NOW()
) ON CONFLICT (id) DO NOTHING;

INSERT INTO merchants (
    id, name, email, api_key, webhook_url,
    status, kyc_status, risk_level, created_at, updated_at
) VALUES (
    '00000000-0000-0000-0000-000000000002',
    'Demo Merchant 2',
    'demo2@paybank.com',
    'pb_test_abcdef1234567890',
    'http://localhost:4004/webhooks',
    'active', 'verified', 'low',
    NOW(), NOW()
) ON CONFLICT (id) DO NOTHING;

INSERT INTO banks (id, name, routing_number, supports_fednow, supports_rtp, supports_wire, logo_url, display_order) VALUES
    ('021000021', 'Chase',           '021000021', TRUE,  TRUE,  TRUE,  NULL, 1),
    ('026009593', 'Bank of America', '026009593', FALSE, TRUE,  TRUE,  NULL, 2),
    ('121000248', 'Wells Fargo',     '121000248', TRUE,  TRUE,  TRUE,  NULL, 3),
    ('021000089', 'Citibank',        '021000089', TRUE,  TRUE,  TRUE,  NULL, 4),
    ('056073502', 'Capital One',     '056073502', TRUE,  TRUE,  TRUE,  NULL, 5),
    ('091000022', 'US Bank',         '091000022', TRUE,  TRUE,  TRUE,  NULL, 6),
    ('043000096', 'PNC Bank',        '043000096', TRUE,  TRUE,  TRUE,  NULL, 7),
    ('031101266', 'TD Bank',         '031101266', FALSE, TRUE,  TRUE,  NULL, 8),
    ('256074974', 'Navy Federal',    '256074974', TRUE,  TRUE,  TRUE,  NULL, 9),
    ('124003116', 'Ally Bank',       '124003116', FALSE, FALSE, FALSE, NULL, 10)
ON CONFLICT (id) DO NOTHING;

INSERT INTO ledger_accounts (id, name, type, merchant_id, currency) VALUES
    ('a0000000-0000-0000-0000-000000000001', 'Cash',          'asset',    '00000000-0000-0000-0000-000000000001', 'USD'),
    ('a0000000-0000-0000-0000-000000000002', 'Receivables',   'asset',    '00000000-0000-0000-0000-000000000001', 'USD'),
    ('a0000000-0000-0000-0000-000000000003', 'Revenue',       'revenue',  '00000000-0000-0000-0000-000000000001', 'USD'),
    ('a0000000-0000-0000-0000-000000000004', 'Settlement',    'liability','00000000-0000-0000-0000-000000000001', 'USD')
ON CONFLICT (id) DO NOTHING;
