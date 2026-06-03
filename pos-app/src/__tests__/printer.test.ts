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
