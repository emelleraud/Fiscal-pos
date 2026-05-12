# Architecture pos-fiscal

Système de caisse enregistreuse certifiable NF525 pour chaînes de restauration rapide.
Architecture offline-first : chaque restaurant fonctionne de manière autonome.

## Diagramme général

```mermaid
graph TB
    subgraph Terminal["Terminal Caisse (Electron + React)"]
        UI[pos-app<br/>React 18 + TypeScript]
    end

    subgraph Edge["Mini-serveur Edge (par restaurant)"]
        API[edge-api<br/>Axum HTTP/LAN]
        FE[fiscal-engine<br/>Journal NF525]
        DB[(SQLite WAL<br/>restaurant.db)]
        SC[sync-client<br/>Agent background]
        API --> FE
        FE --> DB
        SC --> DB
    end

    subgraph Cloud["Cloud (Supabase / PostgreSQL)"]
        SB[(Supabase<br/>PostgreSQL)]
        BO[Back-office<br/>React 18 + TypeScript]
        SB --> BO
    end

    subgraph TPE["TPE Carte Bancaire"]
        TPE_HW[Terminal paiement<br/>Pass-through]
    end

    UI -->|HTTP/LAN| API
    UI -->|Callback| TPE_HW
    TPE_HW -->|Callback| API
    SC -->|HTTP/2 batch push| SB
    SB -->|Config pull| SC
```

## Flux d'une vente (chemin critique)

```mermaid
sequenceDiagram
    participant C as Caisse (React)
    participant A as edge-api
    participant F as fiscal-engine
    participant D as SQLite
    participant T as TPE

    C->>A: POST /api/v1/orders (panier)
    A->>F: record_transaction(Sale)
    F->>D: INSERT fiscal_entries (append-only)
    F-->>A: FiscalEntry { id, hash, sequence }
    A-->>C: 201 { order_id, fiscal_entry_id }

    C->>T: Demande paiement CB
    T-->>C: Callback succès
    C->>A: POST /api/v1/orders/:id/pay
    A->>F: record_transaction(Sale confirmed)
    F->>D: INSERT (hash chaîné)
    A-->>C: 200 { ticket, fiscal_entry_id }
```

## Modèle de données fiscal (SQLite)

```mermaid
erDiagram
    SESSIONS {
        uuid id PK
        integer sequence_number
        timestamp opened_at
        timestamp closed_at
        blob closing_hash
        text status
    }
    FISCAL_ENTRIES {
        uuid id PK
        integer sequence_number
        uuid session_id FK
        text operation_type
        integer amount_ttc_cents
        integer amount_ht_cents
        integer tva_5_5_cents
        integer tva_10_cents
        integer tva_20_cents
        blob hash
        blob previous_hash
        timestamp created_at
        boolean synced
    }
    Z_REPORTS {
        uuid id PK
        uuid session_id FK
        timestamp generated_at
        blob closing_hash
        integer total_sales_cents
        integer total_refunds_cents
        text csv_path
    }
    SESSIONS ||--o{ FISCAL_ENTRIES : "contient"
    SESSIONS ||--o| Z_REPORTS : "clôturé par"
```

## Décisions d'architecture

| Décision | Choix | ADR |
|----------|-------|-----|
| Moteur fiscal | Rust | [ADR-001](adr/001-rust-fiscal-engine.md) |
| API edge | Axum | (ADR-002, Étape 6) |
| Base locale | SQLite WAL | (ADR-003, Étape 4) |
| Sync cloud | Supabase HTTP | (ADR-004, Étape 7) |
| Représentation monétaire | `Cents` (i64) | In-code, `common::Cents` |

## Contraintes de sécurité

- **Aucune donnée bancaire** ne transite par le système : le TPE est pass-through
- **LAN uniquement** : l'`edge-api` ne doit jamais être exposée sur Internet
- **Append-only** : le journal fiscal est en lecture seule après écriture (garanti par Rust + contraintes SQLite)
- **Synchronisation read-only sur le journal** : le `sync-client` ne modifie jamais `fiscal_entries`
