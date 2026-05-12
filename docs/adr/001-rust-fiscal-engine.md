# ADR-001 — Choix de Rust pour le moteur fiscal

**Date :** 2024-01  
**Statut :** Accepté  
**Décideur :** Angelo (DSI / Product Owner)

---

## Contexte

Le moteur fiscal est le composant le plus critique du système. Il doit :

1. Garantir l'**immuabilité** du journal (NF525 §4) : aucune entrée ne peut être modifiée ou supprimée
2. Maintenir une **chaîne de hash SHA-256** vérifiable par un auditeur externe
3. Fonctionner de manière **fiable en conditions dégradées** (panne réseau, redémarrage brutal)
4. Être **certifiable** par un laboratoire accrédité (Infocert/LNE), ce qui implique une lisibilité du code source lors de l'audit
5. Tourner sur du **matériel modeste** (mini-PC à 150€, 4 Go RAM)

L'équipe est composée d'un seul développeur assisté par Claude comme outil de développement principal.

---

## Alternatives considérées

### Option A — Go
**Avantages :** Compilation rapide, runtime léger, bonne gestion des goroutines pour la sync.  
**Inconvénients :**  
- Garbage collector : pauses GC imprévisibles inacceptables dans un contexte transactionnel critique  
- Pas de garanties de sécurité mémoire au niveau type system  
- L'absence de types somme (enums algébriques) rend l'expression des états fiscaux moins précise  

### Option B — Java / Kotlin (JVM)
**Avantages :** Maturité, écosystème riche, Spring Boot bien connu.  
**Inconvénients :**  
- JVM = 200-400 Mo RAM minimum → incompatible avec le matériel cible  
- Temps de démarrage à froid (> 2 secondes) inacceptable pour une caisse  
- Overhead de déploiement (JRE requis sur chaque terminal)  

### Option C — Python
**Avantages :** Vitesse de développement, bibliothèques disponibles.  
**Inconvénients :**  
- Interprété : performance insuffisante pour les calculs de hash en boucle  
- Absence de garanties statiques pour les contraintes d'immuabilité  
- `None` partout : difficile de garantir l'absence de null pointer dans les chemins fiscaux  

### Option D — Rust ✅
**Avantages :**  
- **Ownership system** : le compilateur garantit statiquement qu'aucune entrée fiscale n'est modifiée après insertion (`&` vs `&mut` au niveau type)  
- **Pas de GC** : latence prévisible, critique pour les transactions en pic d'activité (midi/soir)  
- **`Result<T, E>` obligatoire** : impossible d'ignorer silencieusement une erreur du journal fiscal  
- **Binaire statique** : déploiement sans dépendance runtime, idéal pour les mini-serveurs Windows/Linux  
- **Performance** : SHA-256 sur 1 000 transactions < 10 ms sur Raspberry Pi 4  
- **Lisibilité pour l'audit** : le code Rust avec doc comments `///` est lisible par un ingénieur LNE non-Rustacean  
- **`#![deny(clippy::pedantic)]`** : niveau de qualité objectivement vérifiable en CI  

**Inconvénients :**  
- Courbe d'apprentissage plus élevée que Go ou Python  
- Temps de compilation plus long (mitigé par le cache CI et les workspaces Cargo)  

---

## Décision

**Rust** est retenu pour `fiscal-engine` et les binaires edge (`edge-api`, `sync-client`).

Les raisons déterminantes sont :
1. L'**ownership system** est la meilleure garantie technique disponible de l'immuabilité du journal, au-delà des contraintes de base de données
2. La **lisibilité des erreurs** (`Result`, `thiserror`, messages explicites) facilite l'audit LNE
3. L'**absence de runtime** simplifie le déploiement sur les serveurs edge hétérogènes du réseau de restaurants

---

## Conséquences

- Le développement fiscal nécessite une maîtrise de Rust (borrow checker, lifetimes pour les references au journal)
- Claude est utilisé comme pair-programmer pour accélérer la production de code Rust correct
- La CI vérifie `clippy::pedantic` à chaque push : tout warning est bloquant
- Les tests d'intégrité de la chaîne de hash sont écrits en Rust natif (pas de framework de test externe)
- Les binaires sont compilés en release statique (`lto = "thin"`, `strip = "symbols"`) pour réduire la taille des artefacts déployés

---

## Références

- [NF 525 — Logiciels de caisse, exigences](https://www.boutique.afnor.org/fr-fr/norme/nf-z42-026/logiciels-de-caisse/fa201090/3085)
- [The Rust Programming Language — Error Handling](https://doc.rust-lang.org/book/ch09-00-error-handling.html)
- [Cargo Workspaces](https://doc.rust-lang.org/book/ch14-03-cargo-workspaces.html)
