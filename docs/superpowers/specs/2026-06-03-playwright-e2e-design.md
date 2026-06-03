# Playwright E2E — pos-app — Design Spec

**Date :** 2026-06-03  
**Scope :** Couverture E2E UI des 3 flows principaux de la caisse POS  
**Statut :** Validé — prêt pour implémentation

---

## Contexte

Le chantier "impression tickets & rapport Z" (b345cc2) a été vérifié manuellement via `/tmp/verify-print.mjs`. Ce spec formalise cette vérification en une suite de tests Playwright reproductibles.

Les tests sont **locaux uniquement** — ils ne sont pas intégrés à la CI GitHub Actions (comme les tests E2E Rust sync qui sont `--ignored`). Ils nécessitent que l'edge-api (port 8080) et le pos-app dev server (port 5175) soient démarrés manuellement.

---

## Décisions d'architecture

| Question | Décision |
|---|---|
| CI ? | Non — tests locaux uniquement |
| Installation Playwright | `@playwright/test` devDependency dans `pos-app/package.json` |
| Structure | Approche C — Page Object Model + serial |
| Port dev server | 5175 (hardcodé dans `vite.config.ts` et `playwright.config.ts`) |

---

## Fichiers créés / modifiés

```
pos-app/
  vite.config.ts                    ← port: 5175 (déjà 5173, à corriger)
  playwright.config.ts              ← nouveau
  e2e/
    pages/
      PosPage.ts                    ← Page Object Model
    pos-flows.spec.ts               ← 3 tests serial
  package.json                      ← @playwright/test devDep + scripts
```

---

## `playwright.config.ts`

```ts
import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  workers: 1,            // force séquentiel (état session partagé)
  timeout: 30_000,
  use: {
    baseURL: 'http://localhost:5175',
    headless: true,
    video: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  projects: [
    { name: 'chromium', use: { ...devices['Desktop Chrome'] } },
  ],
  // Pas de webServer — serveurs lancés manuellement
});
```

---

## `package.json` — scripts et dépendances

```json
"scripts": {
  "test:e2e": "playwright test",
  "test:e2e:headed": "playwright test --headed"
},
"devDependencies": {
  "@playwright/test": "^1.44.0"
}
```

---

## `e2e/pages/PosPage.ts` — Page Object Model

### Responsabilité

Encapsuler tous les sélecteurs et interactions UI. Les specs lisent comme un scénario métier.

### Méthodes publiques

| Méthode | Description |
|---|---|
| `ensureSessionOpen()` | Détecte "Ouvrir la caisse" → clique si présent, attend OrderScreen |
| `addItem(name: string)` | Clique sur l'article par texte dans la grille produits |
| `submitCart()` | Clique "Encaisser …" → attend PaymentScreen |
| `selectPaymentMethod(m)` | Clique le bouton CB / Espèces / Ticket restaurant |
| `confirmPayment()` | Clique "Paiement reçu — valider" → attend TicketScreen |
| `clickPrint()` | Clique "Imprimer" (fonctionne sur TicketScreen ET ZReportScreen) |
| `newOrder()` | Clique "Nouvelle commande" → attend OrderScreen |
| `navigateToCancel()` | Clique "Annulation" dans le Header → attend CancelScreen |
| `selectCancelReason(r)` | Clique le motif preset correspondant |
| `confirmCancel()` | Coche la checkbox de confirmation + clique "Confirmer l'annulation" |
| `navigateToZReport()` | Clique "Rapport Z" dans le Header → attend ZReportScreen |
| `closeSession()` | Clique "Clôturer et générer le rapport Z" → attend résultats |

### Sélecteurs clés (issus de verify-print.mjs)

```
"Ouvrir la caisse"                     // LoginScreen
"Encaisser"                            // CartPanel → PaymentScreen
"Carte bancaire"                       // PaymentScreen moyen de paiement
"Paiement reçu — valider"              // PaymentScreen bouton confirm
"Paiement accepté"                     // TicketScreen titre
"Imprimer"                             // TicketScreen + ZReportScreen
"Nouvelle commande"                    // TicketScreen action
"Annulation"                           // Header button (toujours visible si session)
"Confirmer l'annulation"               // CancelScreen bouton
"Rapport Z"                            // Header button (visible si session + onZReport)
"Clôturer et générer le rapport Z"     // ZReportScreen bouton
"Session clôturée"                     // ZReportScreen résultat
```

### Capture console pour `[DEV PRINT]`

```ts
private _consoleLogs: string[] = [];

constructor(readonly page: Page) {
  page.on('console', (msg) => {
    this._consoleLogs.push(msg.text());
  });
}

get consoleLogs(): string[] {
  return this._consoleLogs;
}

clearConsoleLogs(): void {
  this._consoleLogs = [];
}
```

---

## `e2e/pos-flows.spec.ts`

```ts
import { test, expect, browser } from '@playwright/test';
import { PosPage } from './pages/PosPage';

test.describe.serial('POS Fiscal — E2E flows', () => {
  let pos: PosPage;

  test.beforeAll(async ({ browser }) => {
    const page = await browser.newPage();
    pos = new PosPage(page);
    await page.goto('/');
    await pos.ensureSessionOpen();
  });

  test('Flow 1 — order → paiement CB → ticket → [DEV PRINT]', async () => {
    await pos.addItem('Burger Classic');
    await pos.submitCart();
    await pos.selectPaymentMethod('card');
    pos.clearConsoleLogs();
    await pos.confirmPayment();
    await expect(pos.page.getByText('Paiement accepté')).toBeVisible();
    await pos.clickPrint();
    expect(pos.consoleLogs.some((l) => l.includes('[DEV PRINT]'))).toBe(true);
    await pos.newOrder();
  });

  test('Flow 2 — paiement CB → ticket → annulation avec motif', async () => {
    await pos.addItem('Burger Classic');
    await pos.submitCart();
    await pos.selectPaymentMethod('card');
    await pos.confirmPayment();
    await expect(pos.page.getByText('Paiement accepté')).toBeVisible();
    // Annuler depuis TicketScreen (currentFiscalEntry encore set)
    await pos.navigateToCancel();
    await pos.selectCancelReason('Erreur de saisie');
    await pos.confirmCancel();
    // Retour sur OrderScreen
    await expect(pos.page.getByText('Caisse')).toBeVisible();
  });

  test('Flow 3 — rapport Z → clôturer → résultats → imprimer', async () => {
    await pos.navigateToZReport();
    await pos.closeSession();
    await expect(pos.page.getByText('Session clôturée')).toBeVisible();
    pos.clearConsoleLogs();
    await pos.clickPrint();
    expect(pos.consoleLogs.some((l) => l.includes('[DEV PRINT]'))).toBe(true);
  });
});
```

---

## Gestion d'état entre tests

| Après | État |
|---|---|
| `beforeAll` | Session ouverte, OrderScreen actif |
| Flow 1 | OrderScreen, panier vide, `currentFiscalEntry` cleared (newOrder) |
| Flow 2 | OrderScreen, panier vide (après annulation) |
| Flow 3 | ZReportScreen résultats, session **fermée** |

**Prochain run :** `beforeAll` détecte l'absence de session → clique "Ouvrir la caisse" → nouvelle session.

---

## Assertions clés

| Assertion | Technique |
|---|---|
| `[DEV PRINT]` déclenché | `page.on('console')` capturé dans PosPage |
| Pas de `window.print()` / `alert()` | Absence dans `consoleLogs` (window.print déclenche un dialog Playwright) |
| Navigation correcte | `expect(page.getByText('...)).toBeVisible()` |
| Retour OrderScreen après cancel | Texte "Caisse" visible dans le Header |

---

## Lancer les tests

```bash
# Prérequis : edge-api et pos-app dev server démarrés
FISCAL_SIGNING_KEY_HEX=8326409b... DATABASE_URL=sqlite:./restaurant.db DATA_DIR=./data cargo run -p edge-api &
cd pos-app && npm run dev &

# Tests E2E
cd pos-app && npm run test:e2e

# Debug visuel
cd pos-app && npm run test:e2e:headed
```

---

## Hors-scope (ce spec)

- Tests en CI (pas de webServer automatique)
- Mode formation (`OperationType::Training` n'existe pas encore)
- Paiement espèces (variante de flow 1 — extension future)
- Rapport Z avec zéro opération
