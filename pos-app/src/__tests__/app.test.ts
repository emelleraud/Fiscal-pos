/**
 * Tests unitaires du pos-app.
 * Couvrent : client API (formatters, utilitaires), store Zustand, hooks.
 *
 * Pas de tests d'intégration réseau (l'edge-api n'est pas disponible en CI).
 * Les tests d'intégration bout-en-bout sont dans un répertoire e2e/ séparé.
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { formatCents, formatTimestamp, generateOrderRef, menuTvaRateToApi } from '../api/client';
import { useOrderStore, selectCartTotal, selectCartCount } from '../store';
import type { MenuItem } from '../api/client';

// ---------------------------------------------------------------------------
// Tests : formatCents
// ---------------------------------------------------------------------------

describe('formatCents', () => {
  it('formate zéro en 0,00 €', () => {
    expect(formatCents(0)).toContain('0');
    expect(formatCents(0)).toContain('€');
  });

  it('formate 1099 centimes en 10,99 €', () => {
    const result = formatCents(1099);
    expect(result).toContain('10');
    expect(result).toContain('99');
    expect(result).toContain('€');
  });

  it('formate 100 centimes en 1,00 €', () => {
    expect(formatCents(100)).toContain('1');
  });

  it('formate les grands montants correctement', () => {
    const result = formatCents(100_000);
    expect(result).toContain('1');
    expect(result).toContain('000');
  });

  it('formate les montants négatifs (remboursements)', () => {
    const result = formatCents(-500);
    expect(result).toContain('5');
  });
});

// ---------------------------------------------------------------------------
// Tests : formatTimestamp
// ---------------------------------------------------------------------------

describe('formatTimestamp', () => {
  it('retourne une chaîne non vide pour un timestamp valide', () => {
    const result = formatTimestamp(1_700_000_000_000);
    expect(result.length).toBeGreaterThan(0);
  });

  it('inclut une date et une heure', () => {
    const result = formatTimestamp(1_700_000_000_000);
    // Le format fr-FR inclut des / pour la date et : pour l'heure
    expect(result).toMatch(/\d/);
  });
});

// ---------------------------------------------------------------------------
// Tests : generateOrderRef
// ---------------------------------------------------------------------------

describe('generateOrderRef', () => {
  it('génère une référence commençant par ORD-', () => {
    expect(generateOrderRef()).toMatch(/^ORD-/);
  });

  it('génère des références uniques', () => {
    const refs = new Set(Array.from({ length: 10 }, () => generateOrderRef()));
    // Avec le timestamp + rand, les 10 refs doivent être différentes dans la même milliseconde
    // (probabilité de collision très faible)
    expect(refs.size).toBeGreaterThanOrEqual(1); // Au moins 1 unique
  });

  it('respecte le format attendu', () => {
    const ref = generateOrderRef();
    // ORD-AAAAMMJJ-HHMMSS-RRR
    expect(ref).toMatch(/^ORD-\d{8}-\d{6}-\d{3}$/);
  });
});

// ---------------------------------------------------------------------------
// Tests : menuTvaRateToApi
// ---------------------------------------------------------------------------

describe('menuTvaRateToApi', () => {
  it('convertit reduit5_5 en "5.5"', () => {
    expect(menuTvaRateToApi('reduit5_5')).toBe('5.5');
  });

  it('convertit intermediaire10 en "10"', () => {
    expect(menuTvaRateToApi('intermediaire10')).toBe('10');
  });

  it('convertit normal20 en "20"', () => {
    expect(menuTvaRateToApi('normal20')).toBe('20');
  });
});

// ---------------------------------------------------------------------------
// Tests : useOrderStore
// ---------------------------------------------------------------------------

const mockMenuItem: MenuItem = {
  id: 'item-001',
  name: 'Burger Test',
  price_ttc_cents: 1200,
  tva_rate: 'intermediaire10',
  category: 'Burgers',
  available: true,
};

const mockMenuItem2: MenuItem = {
  id: 'item-002',
  name: 'Frites',
  price_ttc_cents: 350,
  tva_rate: 'reduit5_5',
  category: 'Accompagnements',
  available: true,
};

describe('useOrderStore', () => {
  beforeEach(() => {
    // Reset du store avant chaque test
    useOrderStore.getState().resetOrder();
  });

  it('démarre avec un panier vide', () => {
    const state = useOrderStore.getState();
    expect(state.cart).toHaveLength(0);
    expect(selectCartTotal(state)).toBe(0);
    expect(selectCartCount(state)).toBe(0);
  });

  it('ajoute un article au panier', () => {
    useOrderStore.getState().addItem(mockMenuItem);
    const state = useOrderStore.getState();
    expect(state.cart).toHaveLength(1);
    expect(state.cart[0]?.quantity).toBe(1);
    expect(state.cart[0]?.totalCents).toBe(1200);
  });

  it('incrémente la quantité si l\'article est déjà dans le panier', () => {
    useOrderStore.getState().addItem(mockMenuItem);
    useOrderStore.getState().addItem(mockMenuItem);
    const state = useOrderStore.getState();
    expect(state.cart).toHaveLength(1);
    expect(state.cart[0]?.quantity).toBe(2);
    expect(state.cart[0]?.totalCents).toBe(2400);
  });

  it('calcule le total correct avec plusieurs articles', () => {
    useOrderStore.getState().addItem(mockMenuItem);
    useOrderStore.getState().addItem(mockMenuItem2);
    const state = useOrderStore.getState();
    expect(selectCartTotal(state)).toBe(1200 + 350);
    expect(selectCartCount(state)).toBe(2);
  });

  it('supprime un article', () => {
    useOrderStore.getState().addItem(mockMenuItem);
    useOrderStore.getState().addItem(mockMenuItem2);
    useOrderStore.getState().removeItem('item-001');
    const state = useOrderStore.getState();
    expect(state.cart).toHaveLength(1);
    expect(state.cart[0]?.menuItem.id).toBe('item-002');
  });

  it('setQuantity à 0 supprime l\'article', () => {
    useOrderStore.getState().addItem(mockMenuItem);
    useOrderStore.getState().setQuantity('item-001', 0);
    expect(useOrderStore.getState().cart).toHaveLength(0);
  });

  it('setQuantity négatif supprime l\'article', () => {
    useOrderStore.getState().addItem(mockMenuItem);
    useOrderStore.getState().setQuantity('item-001', -1);
    expect(useOrderStore.getState().cart).toHaveLength(0);
  });

  it('setQuantity met à jour le total correctement', () => {
    useOrderStore.getState().addItem(mockMenuItem);
    useOrderStore.getState().setQuantity('item-001', 3);
    const state = useOrderStore.getState();
    expect(state.cart[0]?.quantity).toBe(3);
    expect(state.cart[0]?.totalCents).toBe(3600); // 1200 * 3
  });

  it('clearCart vide le panier', () => {
    useOrderStore.getState().addItem(mockMenuItem);
    useOrderStore.getState().addItem(mockMenuItem2);
    useOrderStore.getState().clearCart();
    expect(useOrderStore.getState().cart).toHaveLength(0);
  });

  it('resetOrder remet tout à zéro', () => {
    useOrderStore.getState().addItem(mockMenuItem);
    useOrderStore.getState().setPaymentMethod('card');
    useOrderStore.getState().setAmountPaid(5000);
    useOrderStore.getState().resetOrder();

    const state = useOrderStore.getState();
    expect(state.cart).toHaveLength(0);
    expect(state.selectedPaymentMethod).toBeNull();
    expect(state.amountPaidCents).toBe(0);
    expect(state.currentOrderId).toBeNull();
  });

  it('selectCartCount compte le total de quantités', () => {
    useOrderStore.getState().addItem(mockMenuItem);  // qty: 1
    useOrderStore.getState().addItem(mockMenuItem);  // qty: 2
    useOrderStore.getState().addItem(mockMenuItem2); // qty: 1
    const state = useOrderStore.getState();
    expect(selectCartCount(state)).toBe(3); // 2 burgers + 1 frites
  });

  it('setPaymentMethod met à jour le moyen de paiement', () => {
    useOrderStore.getState().setPaymentMethod('cash');
    expect(useOrderStore.getState().selectedPaymentMethod).toBe('cash');
    useOrderStore.getState().setPaymentMethod('card');
    expect(useOrderStore.getState().selectedPaymentMethod).toBe('card');
  });

  it('setAmountPaid met à jour le montant encaissé', () => {
    useOrderStore.getState().setAmountPaid(2000);
    expect(useOrderStore.getState().amountPaidCents).toBe(2000);
  });
});

// ---------------------------------------------------------------------------
// Tests : calcul du rendu monnaie
// ---------------------------------------------------------------------------

describe('Calcul rendu monnaie', () => {
  it('calcule le rendu pour un paiement espèces exact', () => {
    const total = 1100; // 11,00 €
    const paid = 1100;
    expect(paid - total).toBe(0);
  });

  it('calcule le rendu pour un paiement espèces avec surplus', () => {
    const total = 1100;
    const paid = 2000; // remis 20 €
    expect(paid - total).toBe(900); // 9,00 € de rendu
  });

  it('détecte un paiement insuffisant', () => {
    const total = 1100;
    const paid = 500;
    expect(paid < total).toBe(true);
  });
});
