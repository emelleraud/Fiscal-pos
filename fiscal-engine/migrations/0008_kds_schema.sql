-- Migration 0008 : schéma KDS (Kitchen Display System)

CREATE TABLE IF NOT EXISTS kds_routing_profiles (
    id          TEXT NOT NULL PRIMARY KEY,
    name        TEXT NOT NULL,
    description TEXT
);

INSERT OR IGNORE INTO kds_routing_profiles (id, name, description) VALUES
    ('normal', 'Service normal', 'Stations polyvalentes, personnel réduit'),
    ('rush',   'Rush',           'Stations spécialisées, flux élevé');

CREATE TABLE IF NOT EXISTS kds_active_profile (
    singleton   INTEGER NOT NULL PRIMARY KEY DEFAULT 1 CHECK (singleton = 1),
    profile_id  TEXT    NOT NULL DEFAULT 'normal'
);

INSERT OR IGNORE INTO kds_active_profile (singleton, profile_id) VALUES (1, 'normal');

CREATE TABLE IF NOT EXISTS kds_stations (
    id                  TEXT    NOT NULL PRIMARY KEY,
    name                TEXT    NOT NULL,
    role                TEXT    NOT NULL CHECK (role IN ('preparation','holding','assembly','ready_board')),
    temperature_group   TEXT    CHECK (temperature_group IN ('hot','cold','other')),
    output_type         TEXT    NOT NULL CHECK (output_type IN ('screen','printer','both')),
    printer_address     TEXT,
    printer_type        TEXT    CHECK (printer_type IN ('tcpip','usb','file')),
    printer_mode        TEXT    CHECK (printer_mode IN ('receipt','linerless_label')),
    paper_width_mm      INTEGER CHECK (paper_width_mm IN (50, 80)),
    fallback_station_id TEXT    REFERENCES kds_stations(id),
    active_in_profiles  TEXT    NOT NULL DEFAULT '["normal"]',
    sort_order          INTEGER NOT NULL DEFAULT 0,
    enabled             INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS kds_routing_rules (
    id          TEXT    NOT NULL PRIMARY KEY,
    profile_id  TEXT    NOT NULL REFERENCES kds_routing_profiles(id),
    rule_type   TEXT    NOT NULL CHECK (rule_type IN ('category','product','tag')),
    match_value TEXT    NOT NULL,
    station_ids TEXT    NOT NULL,
    priority    INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS kds_channel_triggers (
    channel     TEXT NOT NULL,
    order_type  TEXT NOT NULL,
    trigger_on  TEXT NOT NULL CHECK (trigger_on IN ('order','payment','both')),
    orb_type    TEXT CHECK (orb_type IN ('client','livreur')),
    PRIMARY KEY (channel, order_type)
);

INSERT OR IGNORE INTO kds_channel_triggers (channel, order_type, trigger_on, orb_type) VALUES
    ('caisse',   'eat_in',          'payment', NULL),
    ('caisse',   'takeaway',        'payment', 'client'),
    ('kiosk',    'eat_in',          'order',   NULL),
    ('kiosk',    'takeaway',        'order',   'client'),
    ('drive',    'drive',           'payment', NULL),
    ('delivery', 'delivery',        'order',   'livreur'),
    ('delivery', 'click_and_collect','order',  'client');

CREATE TABLE IF NOT EXISTS kds_timer_thresholds (
    station_id    TEXT    NOT NULL PRIMARY KEY REFERENCES kds_stations(id),
    warning_secs  INTEGER NOT NULL DEFAULT 120,
    critical_secs INTEGER NOT NULL DEFAULT 300
);

CREATE TABLE IF NOT EXISTS kds_orders (
    order_id           TEXT    NOT NULL,
    station_id         TEXT    NOT NULL,
    order_number_short TEXT    NOT NULL,
    external_order_id  TEXT,
    channel            TEXT    NOT NULL,
    order_type         TEXT    NOT NULL,
    customer_name      TEXT,
    status             TEXT    NOT NULL DEFAULT 'new',
    stage              TEXT    NOT NULL DEFAULT 'preparation',
    station_statuses   TEXT,
    arrived_at         INTEGER NOT NULL,
    first_bump_at      INTEGER,
    ready_at           INTEGER,
    served_at          INTEGER,
    PRIMARY KEY (order_id, station_id)
);

CREATE TABLE IF NOT EXISTS kds_order_lines (
    order_id         TEXT    NOT NULL,
    line_id          TEXT    NOT NULL,
    station_id       TEXT    NOT NULL,
    product_name     TEXT    NOT NULL,
    quantity         INTEGER NOT NULL DEFAULT 1,
    parent_line_id   TEXT,
    line_type        TEXT    NOT NULL CHECK (line_type IN ('item','combo_component','modifier','comment')),
    comment          TEXT,
    acknowledged     INTEGER NOT NULL DEFAULT 0,
    acknowledged_at  INTEGER,
    PRIMARY KEY (order_id, line_id, station_id)
);

CREATE TABLE IF NOT EXISTS kds_failover_log (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    ts                  INTEGER NOT NULL,
    order_id            TEXT    NOT NULL,
    primary_station_id  TEXT    NOT NULL,
    fallback_station_id TEXT    NOT NULL,
    reason              TEXT
);

CREATE TABLE IF NOT EXISTS kds_print_log (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    ts         INTEGER NOT NULL,
    order_id   TEXT    NOT NULL,
    station_id TEXT    NOT NULL,
    attempt    INTEGER NOT NULL DEFAULT 1,
    result     TEXT    NOT NULL,
    error_msg  TEXT
);
