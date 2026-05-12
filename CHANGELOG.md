# Changelog

Toutes les modifications notables de ce projet sont documentées ici.

Format : [Keep a Changelog](https://keepachangelog.com/fr/1.0.0/)
Versioning : [Semantic Versioning](https://semver.org/spec/v2.0.0.html)

---

## [Unreleased]

### Ajouté
- Étape 1 : setup workspace Cargo multi-crates (`fiscal-engine`, `edge-api`, `sync-client`, `common`)
- CI GitHub Actions : fmt → clippy → test → build release
- Crate `common` : newtypes d'identifiants, `Cents`, constantes NF525, `ApiError`
- `.env.example` documenté avec toutes les variables requises
- `/docs/architecture.md` avec diagramme Mermaid
- `/docs/adr/001-rust-fiscal-engine.md` : justification du choix Rust

---

## [0.1.0] — À venir (post-Étape 5)
