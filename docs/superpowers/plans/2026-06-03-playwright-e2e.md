# Playwright E2E — pos-app Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Mettre en place 3 tests Playwright E2E (order→ticket→print, annulation post-paiement, rapport Z) dans pos-app via un Page Object Model.

**Architecture:** `PosPage` (POM, `e2e/pages/PosPage.ts`) + `test.describe.serial` dans `e2e/pos-flows.spec.ts`. Tests locaux uniquement — serveurs lancés manuellement. `@playwright/test` installé comme devDependency.

**Tech Stack:** Playwright 1.60.x, TypeScript strict, Vite 5 dev server port 5175

---

### Task 1: Infrastructure — port Vite, @playwright/test, configs

**Files:**
- Modify: `pos-app/vite.config.ts`
- Modify: `pos-app/package.json`
- Modify: `pos-app/vitest.config.ts`
- Modify: `pos-app/tsconfig.json`
- Create: `pos-app/playwright.config.ts`

- [ ] **Step 1.1: Corriger le port Vite (5173 → 5175) et ajouter strictPort**

Dans `pos-app/vite.config.ts`, modifier la section `server` :

```ts
  server: {
    port: 5175,
    strictPort: true,
    // Proxy vers l'edge-api locale — évite les problèmes CORS en dev
    proxy: {
      '/api': {
        target: 'http://localhost:8080',
        changeOrigin: true,
      },
    },
  },
```

- [ ] **Step 1.2: Ajouter @playwright/test et les scripts dans package.json**

Dans la section `"scripts"` de `pos-app/package.json`, ajouter après `"test:watch"` :
```json
"test:e2e": "playwright test",
"test:e2e:headed": "playwright test --headed",
```

Dans la section `"devDependencies"`, ajouter :
```json
"@playwright/test": "^1.60.0",
```

- [ ] **Step 1.3: Installer les dépendances**

```bash
cd pos-app && npm install
```

Sortie attendue : `added X packages` (ou `up to date` + @playwright/test apparaît dans node_modules).

- [ ] **Step 1.4: Installer Chromium (navigateur Playwright)**

```bash
cd pos-app && npx playwright install chromium
```

Sortie attendue : `Chromium X.X downloaded to ...` ou `Chromium X.X is already installed`.

- [ ] **Step 1.5: Créer playwright.config.ts**

Créer `pos-app/playwright.config.ts` :

```ts
import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  workers: 1,
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
});
```

- [ ] **Step 1.6: Exclure e2e/ de vitest pour éviter les collisions**

Dans `pos-app/vitest.config.ts`, ajouter `exclude` dans la clé `test` :

```ts
import { defineConfig } from 'vitest/config';
import path from 'path';

export default defineConfig({
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: [],
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
    exclude: ['e2e/**', 'node_modules/**'],
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
});
```

- [ ] **Step 1.7: Inclure e2e/ dans tsconfig.json pour que typecheck couvre les tests**

Dans `pos-app/tsconfig.json`, modifier la clé `"include"` :

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "useDefineForClassFields": true,
    "lib": ["ES2020", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "skipLibCheck": true,
    "moduleResolution": "bundler",
    "allowImportingTsExtensions": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx",
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true,
    "baseUrl": ".",
    "paths": {
      "@/*": ["src/*"]
    }
  },
  "include": ["src", "electron", "e2e"],
  "exclude": ["node_modules", "dist"]
}
```

- [ ] **Step 1.8: Vérifier que les tests Vitest passent toujours (pas de collision)**

```bash
cd pos-app && npm test
```

Sortie attendue : `Test Files  2 passed (2)` — les fichiers `printer.test.ts` et `app.test.ts` uniquement, pas de `pos-flows.spec.ts`.

- [ ] **Step 1.9: Vérifier typecheck (ne doit pas échouer sur playwright.config.ts)**

```bash
cd pos-app && npm run typecheck
```

Sortie attendue : aucune erreur TypeScript. (Les fichiers `e2e/` seront vérifiés dès la tâche 2.)

- [ ] **Step 1.10: Commit**

```bash
git add pos-app/vite.config.ts pos-app/package.json pos-app/package-lock.json pos-app/playwright.config.ts pos-app/vitest.config.ts pos-app/tsconfig.json
git commit -m "chore(pos-app): add @playwright/test 1.60, port 5175, playwright.config.ts"
```

---

### Task 2: Page Object Model — e2e/pages/PosPage.ts

**Files:**
- Create: `pos-app/e2e/pages/PosPage.ts`

- [ ] **Step 2.1: Créer pos-app/e2e/pages/PosPage.ts**

```ts
import type { Page, ConsoleMessage } from '@playwright/test';

type PaymentMethod = 'card' | 'cash' | 'meal_voucher';

const PAYMENT_LABELS: Record<PaymentMethod, string> = {
  card: 'Carte bancaire',
  cash: 'Espèces',
  meal_voucher: 'Ticket restaurant',
};

export class PosPage {
  private _consoleLogs: string[] = [];

  constructor(readonly page: Page) {
    page.on('console', (msg: ConsoleMessage) => {
      this._consoleLogs.push(msg.text());
    });
  }

  get consoleLogs(): string[] {
    return this._consoleLogs;
  }

  clearConsoleLogs(): void {
    this._consoleLogs = [];
  }

  async ensureSessionOpen(): Promise<void> {
    await this.page.waitForLoadState('networkidle');
    const openBtn = this.page.getByRole('button', { name: 'Ouvrir la caisse' });
    if (await openBtn.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await openBtn.click();
    }
    await this.page.getByText('Caisse').first().waitFor({ state: 'visible', timeout: 10_000 });
  }

  async addItem(name: string): Promise<void> {
    await this.page.getByText(name).first().click();
  }

  async submitCart(): Promise<void> {
    await this.page.getByRole('button', { name: /Encaisser/ }).click();
    await this.page.getByText('À encaisser').waitFor({ state: 'visible', timeout: 10_000 });
  }

  async selectPaymentMethod(method: PaymentMethod): Promise<void> {
    await this.page.getByText(PAYMENT_LABELS[method]).first().click();
  }

  async confirmPayment(): Promise<void> {
    await this.page.getByRole('button', { name: /Paiement reçu/ }).click();
    await this.page.getByText('Paiement accepté').waitFor({ state: 'visible', timeout: 10_000 });
  }

  // Clique "Imprimer" ou "Imprimer rapport Z" et attend le log [DEV PRINT].
  async clickPrint(): Promise<void> {
    const devPrintPromise = this.page.waitForEvent('console', {
      predicate: (msg) => msg.text().includes('[DEV PRINT]'),
      timeout: 5_000,
    });
    await this.page.getByRole('button', { name: /Imprimer/ }).click();
    await devPrintPromise;
  }

  async newOrder(): Promise<void> {
    await this.page.getByRole('button', { name: 'Nouvelle commande' }).click();
    await this.page.getByText('Caisse').first().waitFor({ state: 'visible', timeout: 5_000 });
  }

  async navigateToCancel(): Promise<void> {
    await this.page.getByRole('button', { name: 'Annulation' }).click();
    await this.page.getByText('Annulation de commande').waitFor({ state: 'visible', timeout: 5_000 });
  }

  async selectCancelReason(reason: string): Promise<void> {
    await this.page.getByRole('button', { name: reason }).click();
  }

  async confirmCancel(): Promise<void> {
    await this.page.getByRole('checkbox').check();
    await this.page.getByRole('button', { name: "Confirmer l'annulation" }).click();
    await this.page.getByText('Caisse').first().waitFor({ state: 'visible', timeout: 10_000 });
  }

  async navigateToZReport(): Promise<void> {
    await this.page.getByRole('button', { name: 'Rapport Z' }).click();
    await this.page.getByRole('heading', { name: 'Rapport Z' }).waitFor({ state: 'visible', timeout: 5_000 });
  }

  async closeSession(): Promise<void> {
    await this.page.getByRole('button', { name: /Clôturer/ }).click();
    await this.page.getByText('Session clôturée').waitFor({ state: 'visible', timeout: 15_000 });
  }
}
```

- [ ] **Step 2.2: Vérifier typecheck (couvre maintenant e2e/)**

```bash
cd pos-app && npm run typecheck
```

Sortie attendue : aucune erreur TypeScript.

- [ ] **Step 2.3: Commit**

```bash
git add pos-app/e2e/pages/PosPage.ts
git commit -m "feat(pos-app): PosPage POM pour les tests Playwright E2E"
```

---

### Task 3: Spec file — e2e/pos-flows.spec.ts

**Files:**
- Create: `pos-app/e2e/pos-flows.spec.ts`

- [ ] **Step 3.1: Créer pos-app/e2e/pos-flows.spec.ts**

```ts
import { test, expect } from '@playwright/test';
import { PosPage } from './pages/PosPage';

test.describe.serial('POS Fiscal — E2E flows', () => {
  let pos: PosPage;

  test.beforeAll(async ({ browser }) => {
    const page = await browser.newPage();
    pos = new PosPage(page);
    await page.goto('/');
    await pos.ensureSessionOpen();
  });

  // Flow 1 : ajouter un article → payer CB → TicketScreen → imprimer
  // Vérifie que [DEV PRINT] apparaît dans la console (printViaElectron en dev).
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

  // Flow 2 : payer une commande → depuis TicketScreen, annuler avec motif
  // Vérifie que l'annulation NF525 est possible depuis le ticket.
  test('Flow 2 — paiement CB → ticket → annulation avec motif', async () => {
    await pos.addItem('Burger Classic');
    await pos.submitCart();
    await pos.selectPaymentMethod('card');
    await pos.confirmPayment();
    await expect(pos.page.getByText('Paiement accepté')).toBeVisible();
    await pos.navigateToCancel();
    await pos.selectCancelReason('Erreur de saisie');
    await pos.confirmCancel();
    await expect(pos.page.getByText('Caisse')).toBeVisible();
  });

  // Flow 3 : clôturer la session → rapport Z → imprimer
  // Doit être le dernier test — la session est fermée après cette opération.
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

- [ ] **Step 3.2: Vérifier typecheck**

```bash
cd pos-app && npm run typecheck
```

Sortie attendue : aucune erreur.

- [ ] **Step 3.3: Vérifier que vitest n'exécute pas pos-flows.spec.ts**

```bash
cd pos-app && npm test
```

Sortie attendue : exactement `2 passed` (`printer.test.ts` + `app.test.ts`). Si `pos-flows.spec.ts` apparaît, vérifier que `vitest.config.ts` a bien `exclude: ['e2e/**', 'node_modules/**']`.

- [ ] **Step 3.4: Commit**

```bash
git add pos-app/e2e/pos-flows.spec.ts
git commit -m "feat(pos-app): spec Playwright E2E — 3 flows (order, cancel, z-report)"
```

---

### Task 4: Lancer les tests E2E et vérifier

**Prérequis :** les deux serveurs doivent être démarrés avant `npm run test:e2e`.

- [ ] **Step 4.1: Démarrer l'edge-api (terminal séparé)**

```bash
cd /home/angelo/PROJ_POS_QSR/pos-fiscal
FISCAL_SIGNING_KEY_HEX=8326409b060929132daf864a66277932c552ea7c9ddfa500189905a2f6a8ae90 \
DATABASE_URL=sqlite:./restaurant.db DATA_DIR=./data cargo run -p edge-api
```

Vérifier : `curl http://localhost:8080/api/v1/health` doit retourner `{"status":"ok"}`.

- [ ] **Step 4.2: Démarrer le pos-app dev server (terminal séparé)**

```bash
cd pos-app && npm run dev
```

Vérifier que la sortie affiche `➜  Local: http://localhost:5175/` (pas 5173 — `strictPort: true` garantit ça).

- [ ] **Step 4.3: Lancer les tests E2E**

```bash
cd pos-app && npm run test:e2e
```

Sortie attendue :
```
Running 3 tests using 1 worker

  ✓  [chromium] › pos-flows.spec.ts:6:3 › POS Fiscal — E2E flows › Flow 1 — order → paiement CB → ticket → [DEV PRINT]
  ✓  [chromium] › pos-flows.spec.ts:6:3 › POS Fiscal — E2E flows › Flow 2 — paiement CB → ticket → annulation avec motif
  ✓  [chromium] › pos-flows.spec.ts:6:3 › POS Fiscal — E2E flows › Flow 3 — rapport Z → clôturer → résultats → imprimer

  3 passed (Xs)
```

- [ ] **Step 4.4: Si un test échoue — debug avec mode headed**

Inspecter visuellement et isoler :

```bash
# Voir tous les tests en browser
cd pos-app && npm run test:e2e:headed

# Isoler un flow par nom
cd pos-app && npx playwright test --headed -g "Flow 1"
cd pos-app && npx playwright test --headed -g "Flow 2"
cd pos-app && npx playwright test --headed -g "Flow 3"
```

Les screenshots d'échec sont dans `pos-app/test-results/`.

**Causes fréquentes et corrections :**

| Symptôme | Cause probable | Correction |
|---|---|---|
| `Timeout waiting for 'À encaisser'` | Menu non chargé (edge-api pas démarrée) | Vérifier `curl :8080/api/v1/health` |
| `Timeout waiting for 'Caisse'` | `ensureSessionOpen()` n'a pas trouvé de session | Vérifier `curl :8080/api/v1/sessions/current` |
| `[DEV PRINT]` non trouvé | Le timeout de `waitForEvent` est trop court | Augmenter le timeout dans `clickPrint()` |
| `Clôturer` button non trouvé dans Flow 3 | Session déjà fermée depuis le run précédent | `beforeAll` doit avoir ré-ouvert une session |
| `'Annulation de commande'` non visible | Pas de `currentFiscalEntry` dans le store | Vérifier que Flow 2 passe bien par `confirmPayment()` avant `navigateToCancel()` |

- [ ] **Step 4.5: Vérifier que les tests Vitest passent toujours**

```bash
cd pos-app && npm test
```

Sortie attendue : `Test Files  2 passed (2)`.

- [ ] **Step 4.6: Ajouter playwright-report/ et test-results/ au .gitignore racine**

Dans `/home/angelo/PROJ_POS_QSR/pos-fiscal/.gitignore`, ajouter :

```gitignore
# Playwright
pos-app/test-results/
pos-app/playwright-report/
```

- [ ] **Step 4.7: Commit final**

```bash
git add .gitignore
git commit -m "chore: ignorer les artefacts Playwright (test-results, playwright-report)"
```
