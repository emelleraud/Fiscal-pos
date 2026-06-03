import { formatCents } from '@/api/client';
import type { CartItem } from '@/store';
import type { AppliedPromo, PaymentMethod } from '@/api/client';

const WIDTH = 40;

function divider(char = '-'): string {
  return char.repeat(WIDTH);
}

function center(text: string): string {
  const clipped = text.slice(0, WIDTH);
  const pad = Math.max(0, Math.floor((WIDTH - clipped.length) / 2));
  return ' '.repeat(pad) + clipped;
}

function row(label: string, value: string): string {
  const gap = WIDTH - label.length - value.length;
  if (gap <= 0) return (label + value).slice(0, WIDTH);
  return label + ' '.repeat(gap) + value;
}

const PAYMENT_LABELS: Record<string, string> = {
  card: 'Carte bancaire',
  cash: 'Espèces',
  meal_voucher: 'Ticket restaurant',
};

export interface TicketParams {
  sessionSequence: number;
  cart: CartItem[];
  appliedPromos: AppliedPromo[];
  totalCents: number;
  netTotal: number;
  paymentMethod: PaymentMethod;
  amountPaidCents: number;
  changeCents: number;
  sequenceNumber: number;
  hashHex: string;
  createdAtMs: number;
}

export function formatTicket(p: TicketParams): string {
  const date = new Date(p.createdAtMs).toLocaleString('fr-FR', {
    dateStyle: 'short',
    timeStyle: 'short',
  });
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
  try {
    const result = await window.electronAPI.printText(text);
    if (!result.success) {
      onError(result.error ?? 'Erreur impression inconnue');
    }
  } catch (err) {
    onError(err instanceof Error ? err.message : 'Erreur impression inconnue');
  }
}
