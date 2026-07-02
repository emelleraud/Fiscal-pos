# Impression tickets & rapport Z — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Câbler `window.electronAPI.printText()` dans `TicketScreen` et `ZReportScreen`, avec un formateur de ticket 40 colonnes plain-text et feedback d'erreur via la StatusBar.

**Architecture:** Un module `src/utils/printer.ts` expose `formatTicket()` (formateur client-side) et `printViaElectron()` (wrapper async avec fallback dev). Les deux écrans abandonnent `window.print()` / `alert()` au profit de ce module. Le texte du rapport Z vient déjà du backend (`lastZReportText` dans le store).

**Tech Stack:** TypeScript, React 18, Zustand, Vitest 1.4, Electron contextBridge (déjà câblé).

---

## Note préalable — `vite-env.d.ts` déjà à jour

`window.electronAPI` est déjà déclaré dans `pos-app/src/vite-env.d.ts` :

```ts
interface ElectronAPI {
  getApiUrl: () => Promise<string>;
  printText: (text: string) => Promise<{ success: boolean; error?: string }>;
}
declare interface Window {
  electronAPI?: ElectronAPI;
}
```

**Aucune modification nécessaire dans ce fichier.**

---

## Fichiers

| Fichier | Nature |
|---|---|
| `pos-app/src/utils/printer.ts` | Créer — `formatTicket` + `printViaElectron` |
| `pos-app/src/__tests__/printer.test.ts` | Créer — tests Vitest sur `formatTicket` |
| `pos-app/src/screens/TicketScreen.tsx` | Modifier — handlers `TicketScreen` et `ZReportScreen` |

---

## Task 1 : Module `printer.ts` (TDD)

**Files:**
- Create: `pos-app/src/utils/printer.ts`
- Test: `pos-app/src/__tests__/printer.test.ts`

- [ ] **Step 1.1 — Écrire les tests (fichier vide requis d'abord)**

Créer `pos-app/src/utils/printer.ts` avec juste l'export vide pour que l'import compile :

```ts
// pos-app/src/utils/printer.ts
export {};
```

Puis créer `pos-app/src/__tests__/printer.test.ts` :

```ts
import { describe, it, expect } from 'vitest';
import { formatTicket } from '../utils/printer';
import type { CartItem } from '../store';
import type { AppliedPromo } from '../api/client';

const mockCart: CartItem[] = [
  {
    menuItem: {
      id: 'item-001',
      name: 'Burger Classic',
      price_ttc_cents: 800,
      tva_rate: 'intermediaire10',
      category: 'Burgers',
      available: true,
    },
    quantity: 2,
    totalCents: 1600,
  },
  {
    menuItem: {
      id: 'item-002',
      name: 'Coca-Cola',
      price_ttc_cents: 250,
      tva_rate: 'reduit5_5',
      category: 'Boissons',
      available: true,
    },
    quantity: 1,
    totalCents: 250,
  },
];

const baseParams = {
  sessionSequence: 42,
  cart: mockCart,
  appliedPromos: [] as AppliedPromo[],
  totalCents: 1850,
  netTotal: 1850,
  paymentMethod: 'card',
  amountPaidCents: 0,
  changeCents: 0,
  sequenceNumber: 127,
  hashHex: 'a3f2c1d4e5b6f7a8',
  createdAtMs: new Date('2026-06-03T14:30:00').getTime(),
};

describe('formatTicket', () => {
  it('toutes les lignes font au plus 40 caractères', () => {
    const text = formatTicket(baseParams);
    text.split('\n').forEach((line) => {
      expect(line.length).toBeLessThanOrEqual(40);
    });
  });

  it('sans promos : pas de section NET A PAYER', () => {
    const text = formatTicket({ ...baseParams, appliedPromos: [] });
    expect(text).not.toContain('NET A PAYER');
  });

  it('avec promos : nom de la promo et NET A PAYER présents', () => {
    const promos: AppliedPromo[] = [
      { promo_id: 'p1', name: 'Happy Hour', discount_cents: 100 },
    ];
    const text = formatTicket({ ...baseParams, appliedPromos: promos, netTotal: 1750 });
    expect(text).toContain('Happy Hour');
    expect(text).toContain('NET A PAYER');
  });

  it('paiement espèces : lignes Remis et Rendu présentes', () => {
    const text = formatTicket({
      ...baseParams,
      paymentMethod: 'cash',
      amountPaidCents: 2000,
      changeCents: 150,
    });
    expect(text).toContain('Remis');
    expect(text).toContain('Rendu');
  });

  it('paiement carte : pas de lignes Remis/Rendu', () => {
    const text = formatTicket({ ...baseParams, paymentMethod: 'card' });
    expect(text).not.toContain('Remis');
    expect(text).not.toContain('Rendu');
  });

  it('pied de ticket : hash tronqué à 8 chars et numéro de séquence présents', () => {
    const text = formatTicket(baseParams);
    expect(text).toContain('Seq #127');
    expect(text).toContain('a3f2c1d4');
  });

  it('pied de ticket : mention NF525 présente', () => {
    const text = formatTicket(baseParams);
    expect(text).toContain('NF525');
  });
});
```

- [ ] **Step 1.2 — Vérifier que les tests échouent**

```bash
cd pos-app && npm test 2>&1 | tail -20
```

Résultat attendu : erreur `formatTicket is not a function` ou similaire.

- [ ] **Step 1.3 — Implémenter `printer.ts`**

Remplacer le contenu de `pos-app/src/utils/printer.ts` par :

```ts
import type { CartItem } from '@/store';
import type { AppliedPromo } from '@/api/client';
import { formatCents } from '@/api/client';

const WIDTH = 40;

function divider(char = '-'): string {
  return char.repeat(WIDTH);
}

function center(text: string): string {
  const pad = Math.max(0, Math.floor((WIDTH - text.length) / 2));
  return ' '.repeat(pad) + text;
}

function row(label: string, value: string): string {
  const gap = WIDTH - label.length - value.length;
  if (gap <= 0) return (label + value).slice(0, WIDTH);
  return label + ' '.repeat(gap) + value;
}

const PAYMENT_LABELS: Record<string, string> = {
  card: 'Carte bancaire',
  cash: 'Especes',
  meal_voucher: 'Ticket restaurant',
};

export interface TicketParams {
  sessionSequence: number;
  cart: CartItem[];
  appliedPromos: AppliedPromo[];
  totalCents: number;
  netTotal: number;
  paymentMethod: string;
  amountPaidCents: number;
  changeCents: number;
  sequenceNumber: number;
  hashHex: string;
  createdAtMs: number;
}

export function formatTicket(p: TicketParams): string {
  const date = new Date(p.createdAtMs).toLocaleDateString('fr-FR');
  const out: string[] = [];

  out.push(divider('='));
  out.push(center('CAISSE POS NF525'));
  out.push(center(`Session #${p.sessionSequence} | ${date}`));
  out.push(divider('='));

  for (const { menuItem, quantity, totalCents } of p.cart) {
    const label = ` ${quantity}x ${menuItem.name}`;
    out.push(row(label, formatCents(totalCents)));
  }

  out.push(divider('-'));
  out.push(row(' TOTAL TTC', formatCents(p.totalCents)));

  if (p.appliedPromos.length > 0) {
    for (const promo of p.appliedPromos) {
      out.push(row(` [${promo.name}]`, `-${formatCents(promo.discount_cents)}`));
    }
    out.push(row(' NET A PAYER', formatCents(p.netTotal)));
  }

  out.push(row(' Mode', PAYMENT_LABELS[p.paymentMethod] ?? p.paymentMethod));
  if (p.paymentMethod === 'cash') {
    out.push(row(' Remis', formatCents(p.amountPaidCents)));
    out.push(row(' Rendu', formatCents(p.changeCents)));
  }

  out.push(divider('='));
  out.push(row(` Seq #${p.sequenceNumber}`, `Hash : ${p.hashHex.slice(0, 8)}`));
  out.push(center('Certifie NF525'));
  out.push(divider('='));
  out.push(center('Merci !'));

  return out.join('\n');
}

export async function printViaElectron(
  text: string,
  onError: (msg: string) => void
): Promise<void> {
  if (!window.electronAPI) {
    console.log('[DEV PRINT]\n' + text);
    return;
  }
  const result = await window.electronAPI.printText(text);
  if (!result.success) {
    onError(result.error ?? 'Erreur impression inconnue');
  }
}
```

- [ ] **Step 1.4 — Vérifier que les tests passent**

```bash
cd pos-app && npm test 2>&1 | tail -20
```

Résultat attendu : tous les tests `formatTicket` verts, suite existante inchangée.

- [ ] **Step 1.5 — Typecheck**

```bash
cd pos-app && npm run typecheck 2>&1
```

Résultat attendu : aucune erreur.

- [ ] **Step 1.6 — Commit**

```bash
cd pos-app && git add src/utils/printer.ts src/__tests__/printer.test.ts
git commit -m "feat(pos-app): add printer module with formatTicket and printViaElectron"
```

---

## Task 2 : Câbler l'impression dans `TicketScreen` et `ZReportScreen`

**Files:**
- Modify: `pos-app/src/screens/TicketScreen.tsx`

Les deux composants (`TicketScreen` et `ZReportScreen`) vivent dans ce fichier.

- [ ] **Step 2.1 — Modifier `TicketScreen`**

En haut du fichier, ajouter l'import :

```ts
import { printViaElectron, formatTicket } from '@/utils/printer';
```

Dans le composant `TicketScreen`, remplacer :

```ts
  const handlePrint = () => {
    window.print();
  };
```

par :

```ts
  const [isPrinting, setIsPrinting] = React.useState(false);
  const setGlobalError = useUiStore((s) => s.setGlobalError);

  const handlePrint = async () => {
    setIsPrinting(true);
    await printViaElectron(
      formatTicket({
        sessionSequence: session?.session_sequence ?? 0,
        cart,
        appliedPromos,
        totalCents,
        netTotal,
        paymentMethod: paymentMethod ?? 'card',
        amountPaidCents: amountPaid,
        changeCents,
        sequenceNumber: entry?.sequence_number ?? 0,
        hashHex: entry?.hash_hex ?? '',
        createdAtMs: entry?.created_at_ms ?? Date.now(),
      }),
      setGlobalError
    );
    setIsPrinting(false);
  };
```

Puis mettre à jour le bouton "Imprimer" dans le JSX (chercher `onClick={handlePrint}` dans la section Actions) :

```tsx
        <Button variant="ghost" size="lg" loading={isPrinting} onClick={() => { void handlePrint(); }}>
          🖨 Imprimer
        </Button>
```

- [ ] **Step 2.2 — Modifier `ZReportScreen`**

Dans le composant `ZReportScreen`, remplacer :

```ts
  const handlePrint = () => {
    if (lastZReportText) {
      // En production : envoyer via IPC Electron au driver imprimante
      alert(lastZReportText);
    }
  };
```

par :

```ts
  const [isPrinting, setIsPrinting] = React.useState(false);
  const setGlobalError = useUiStore((s) => s.setGlobalError);

  const handlePrint = async () => {
    if (!lastZReportText) return;
    setIsPrinting(true);
    await printViaElectron(lastZReportText, setGlobalError);
    setIsPrinting(false);
  };
```

Puis mettre à jour le bouton "Imprimer rapport Z" dans le JSX (chercher l'appel `onClick={handlePrint}` dans la section Actions) :

```tsx
          <Button variant="secondary" size="lg" loading={isPrinting} onClick={() => { void handlePrint(); }}>
            🖨 Imprimer rapport Z
          </Button>
```

- [ ] **Step 2.3 — Typecheck**

```bash
cd pos-app && npm run typecheck 2>&1
```

Résultat attendu : aucune erreur TypeScript.

- [ ] **Step 2.4 — Tests**

```bash
cd pos-app && npm test 2>&1 | tail -20
```

Résultat attendu : tous les tests passent (suite existante + nouveaux tests printer).

- [ ] **Step 2.5 — Commit**

```bash
cd pos-app && git add src/screens/TicketScreen.tsx
git commit -m "feat(pos-app): wire printViaElectron in TicketScreen and ZReportScreen"
```

---

## Task 3 : Vérification finale

- [ ] **Step 3.1 — CI locale complète**

```bash
cd /home/angelo/PROJ_POS_QSR/pos-fiscal
cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo test --workspace && cargo build --release
```

Résultat attendu : sortie `Finished release` sans erreur (le Rust n'a pas changé — vérification de non-régression).

```bash
cd pos-app && npm run typecheck && npm test
```

Résultat attendu : 0 erreur TypeScript, tous les tests verts.

- [ ] **Step 3.2 — Vérifier le comportement en dev (sans Electron)**

```bash
cd pos-app && npm run dev
```

Ouvrir `http://localhost:5173`, ouvrir une session (l'edge-api doit tourner) ou observer le comportement login.

Cliquer sur "Imprimer" depuis le TicketScreen après un paiement : vérifier dans la console du navigateur qu'on voit `[DEV PRINT]` suivi du texte 40 colonnes, **sans** `alert()` ni `window.print()`.

- [ ] **Step 3.3 — Commit final (si modifs de propreté nécessaires)**

Si aucune modification supplémentaire, ce step est no-op. Sinon :

```bash
cd pos-app && git add -p
git commit -m "chore(pos-app): cleanup after print wiring"
```
