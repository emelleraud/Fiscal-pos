-- Migration 0009 : lignes KDS en attente de dispatch (bridge order→pay)
-- Stocke les données d'affichage KDS entre POST /orders et POST /pay.
-- Nettoyées après dispatch dans pay_order_handler (best-effort).

CREATE TABLE IF NOT EXISTS order_pending_lines (
    order_id    TEXT    NOT NULL,
    line_id     TEXT    NOT NULL,
    label       TEXT    NOT NULL,
    product_id  TEXT    NOT NULL DEFAULT '',
    category    TEXT    NOT NULL DEFAULT '',
    quantity    INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (order_id, line_id)
);
