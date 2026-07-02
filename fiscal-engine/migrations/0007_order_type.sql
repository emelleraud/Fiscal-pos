-- Migration 0007 : order_type pour le routage KDS
-- Crée la table orders avec la colonne order_type si elle n'existe pas encore,
-- ou ajoute la colonne à une table orders existante.
CREATE TABLE IF NOT EXISTS orders (
    id         TEXT NOT NULL PRIMARY KEY,
    order_type TEXT NOT NULL DEFAULT 'eat_in'
               CHECK (order_type IN ('eat_in','takeaway','click_and_collect','delivery','drive')),
    created_at_ms INTEGER NOT NULL DEFAULT 0
);
