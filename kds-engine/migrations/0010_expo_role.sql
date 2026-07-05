-- Migration 0010 : ajoute 'expo' au CHECK de kds_stations.role
-- SQLite ne supporte pas ALTER CHECK — recréation de la table nécessaire.

PRAGMA foreign_keys = OFF;

DROP TABLE IF EXISTS kds_stations_new;

CREATE TABLE kds_stations_new (
    id                  TEXT    NOT NULL PRIMARY KEY,
    name                TEXT    NOT NULL,
    role                TEXT    NOT NULL CHECK (role IN ('preparation','holding','assembly','ready_board','expo')),
    temperature_group   TEXT    CHECK (temperature_group IN ('hot','cold','other')),
    output_type         TEXT    NOT NULL CHECK (output_type IN ('screen','printer','both')),
    printer_address     TEXT,
    printer_type        TEXT    CHECK (printer_type IN ('tcpip','usb','file')),
    printer_mode        TEXT    CHECK (printer_mode IN ('receipt','linerless_label')),
    paper_width_mm      INTEGER CHECK (paper_width_mm IN (50, 80)),
    fallback_station_id TEXT    REFERENCES kds_stations_new(id),
    active_in_profiles  TEXT    NOT NULL DEFAULT '["normal"]',
    sort_order          INTEGER NOT NULL DEFAULT 0,
    enabled             INTEGER NOT NULL DEFAULT 1
);

INSERT INTO kds_stations_new SELECT * FROM kds_stations;

DROP TABLE kds_stations;

ALTER TABLE kds_stations_new RENAME TO kds_stations;

PRAGMA foreign_keys = ON;
