# Spec — Impression tickets & rapport Z depuis pos-app

**Date :** 2026-06-03  
**Périmètre :** pos-app uniquement (TypeScript/React/Electron)  
**Chantier :** Câbler `window.electronAPI.printText()` dans `TicketScreen` et `ZReportScreen`, créer un formateur de ticket 40 colonnes.

---

## Contexte

Le preload Electron expose `window.electronAPI.printText(text)` depuis le début du projet. Le process principal (`electron/main.ts`) gère déjà l'IPC `print-text` (log console en MVP, ESC/POS en production future).

Deux écrans ont un bouton "Imprimer" qui n'utilise pas encore cette API :
- `TicketScreen` → appelle `window.print()` (impression navigateur, inutile en kiosk)
- `ZReportScreen` → appelle `alert(lastZReportText)` (placeholder)

Le texte du rapport Z est déjà produit par le backend Rust (`CloseSessionResponse.printable_text`) et stocké dans `useUiStore.lastZReportText`. Seul le ticket nécessite un formateur côté client.

---

## Fichiers modifiés

| Fichier | Nature |
|---|---|
| `src/vite-env.d.ts` | Ajout déclaration `window.electronAPI` |
| `src/utils/printer.ts` | Nouveau module : `printViaElectron` + `formatTicket` |
| `src/screens/TicketScreen.tsx` | `handlePrint` dans `TicketScreen` et `ZReportScreen` |
| `src/__tests__/printer.test.ts` | Nouveau — tests Vitest sur `formatTicket` |

Aucun changement Rust, aucun changement edge-api, aucun changement backoffice.

---

## Types TypeScript — `vite-env.d.ts`

```ts
interface Window {
  electronAPI?: {
    getApiUrl: () => Promise<string>;
    printText: (text: string) => Promise<{ success: boolean; error?: string }>;
  };
}
```

L'interface est `optional` (`?`) : en mode dev React pur (sans Electron), `window.electronAPI` est absent. `printViaElectron` gère ce cas avec un fallback `console.log`.

---

## Module `src/utils/printer.ts`

### `printViaElectron(text, onError)`

```ts
export async function printViaElectron(
  text: string,
  onError: (msg: string) => void
): Promise<void>
```

- Si `window.electronAPI` est absent (dev sans Electron) → `console.log('[DEV PRINT]\n' + text)` et retour silencieux.
- Sinon → `await window.electronAPI.printText(text)`.
- Si `result.success === false` → appelle `onError(result.error ?? 'Erreur impression inconnue')`.

### `formatTicket(params)`

Formateur 40 colonnes plain-text. Paramètres :

```ts
interface TicketParams {
  sessionSequence: number;
  cart: CartItem[];
  appliedPromos: AppliedPromo[];
  totalCents: number;       // brut avant remises
  netTotal: number;         // après remises
  paymentMethod: string;
  amountPaidCents: number;  // espèces seulement
  changeCents: number;      // rendu monnaie (0 si carte)
  sequenceNumber: number;   // séquence NF525 de l'entrée fiscale
  hashHex: string;
  createdAtMs: number;
}
```

Sortie cible (40 chars) :

```
========================================
         CAISSE POS NF525
        Session #42 | 03/06/2026
========================================
 2x Burger Classic          10,00 EUR
 1x Coca-Cola                2,50 EUR
----------------------------------------
 TOTAL TTC                  12,50 EUR
 [Happy Hour]               -1,00 EUR
 NET A PAYER                11,50 EUR
 Mode : Especes
 Remis                      15,00 EUR
 Rendu                       3,50 EUR
========================================
 Seq #127  Hash : a3f2c1d4
 Certifie NF525
========================================
           Merci !
```

Règles de formatage :
- Largeur fixe : **40 caractères** par ligne.
- Helper interne `row(label, value, width=40)` : pad label à gauche, value à droite.
- Section "Remises" absente si `appliedPromos` est vide.
- Section "Remis / Rendu" absente si `paymentMethod !== 'cash'`.
- Hash tronqué aux 8 premiers caractères.
- Dates en `DD/MM/YYYY` via `toLocaleDateString('fr-FR')`.
- Montants via `formatCents()` importé depuis `@/api/client`.

---

## Modifications des handlers

### `TicketScreen.handlePrint`

Avant :
```ts
const handlePrint = () => { window.print(); };
```

Après :
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

Bouton "Imprimer" : ajouter `loading={isPrinting}` (prop déjà supportée par `Button`).

### `ZReportScreen.handlePrint`

Avant :
```ts
const handlePrint = () => {
  if (lastZReportText) alert(lastZReportText);
};
```

Après :
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

Bouton "Imprimer rapport Z" : ajouter `loading={isPrinting}`.

---

## Tests Vitest — `src/__tests__/printer.test.ts`

Tests sur `formatTicket` uniquement (pas de mock Electron) :

| Test | Assertion |
|---|---|
| Largeur 40 colonnes | Chaque ligne de la sortie fait ≤ 40 caractères |
| Sans promos | Pas de section "Remises" dans la sortie |
| Avec promos | Section "Remises" présente, remise correctement formatée |
| Paiement espèces | Lignes "Remis" et "Rendu" présentes |
| Paiement carte | Lignes "Remis" et "Rendu" absentes |
| Hash et séquence | Sous-chaînes `Seq #` et `Hash :` présentes en pied de ticket |
| Pied NF525 | Sous-chaîne `Certifie NF525` présente |

---

## Hors périmètre

- Driver ESC/POS réel dans `electron/main.ts` (MVP : console log, prévu plus tard)
- Formatage côté client du rapport Z (le texte vient du backend Rust)
- Mode formation (`OperationType::Training`)
- Tests Playwright
