/**
 * PaymentScreen — sélection du moyen de paiement et confirmation.
 *
 * Flux :
 * 1. Affichage du montant à encaisser
 * 2. Sélection du moyen de paiement (CB, Espèces, Ticket resto)
 * 3. Pour les espèces : saisie du montant remis → calcul du rendu monnaie
 * 4. Confirmation → PaymentResponse → TicketScreen
 *
 * Le paiement CB est pass-through : le TPE est activé physiquement par le
 * caissier, puis la confirmation arrive via le callback de l'edge-api.
 * En MVP, la confirmation est simulée par le bouton "Paiement reçu".
 */

import React, { useState } from 'react';
import { useOrder } from '@/hooks/useOrder';
import { useOrderStore, useUiStore, selectNetTotal, selectDiscountTotal } from '@/store';
import { Header, StatusBar } from '@/components/layout/Header';
import { Button } from '@/components/ui';
import { formatCents } from '@/api/client';

type Method = 'card' | 'cash' | 'meal_voucher';

// Boutons numériques pour la saisie des espèces
const CASH_PRESETS = [500, 1000, 2000, 5000]; // centimes : 5€, 10€, 20€, 50€

export function PaymentScreen(): React.ReactElement {
  const order = useOrder();
  const navigateTo = useUiStore((s) => s.navigateTo);
  const netTotalCents = useOrderStore(selectNetTotal);
  const discountTotalCents = useOrderStore(selectDiscountTotal);
  const appliedPromos = useOrderStore((s) => s.appliedPromos);

  // Saisie espèces
  const [cashInput, setCashInput] = useState('');

  const totalCents = netTotalCents;
  const selectedMethod = order.selectedPaymentMethod;

  const cashAmountCents = cashInput
    ? Math.round(parseFloat(cashInput.replace(',', '.')) * 100)
    : 0;
  const changeCents = cashAmountCents - totalCents;
  const cashInsufficient = selectedMethod === 'cash' && cashAmountCents < totalCents;

  const handleConfirm = () => {
    if (selectedMethod === 'cash') {
      order.setAmountPaid(cashAmountCents);
    } else {
      order.setAmountPaid(totalCents);
    }
    order.confirmPayment();
  };

  const canConfirm =
    selectedMethod !== null &&
    !order.isConfirmingPayment &&
    (selectedMethod !== 'cash' || !cashInsufficient);

  return (
    <div className="h-screen flex flex-col bg-gray-900 text-white">
      <Header title="Paiement" />

      <main className="flex-1 flex items-start justify-center pt-12 px-6 gap-8">
        {/* Colonne gauche : montant + méthodes */}
        <div className="flex flex-col gap-6 w-96">
          {/* Montant */}
          <div className="bg-gray-800 rounded-2xl p-6 text-center">
            <p className="text-gray-400 text-sm mb-1">À encaisser</p>
            <p className="text-4xl font-bold text-white">{formatCents(totalCents)}</p>
            {discountTotalCents > 0 && (
              <div className="mt-3 space-y-1">
                {appliedPromos.map((p) => (
                  <p key={p.promo_id} className="text-green-400 text-sm">
                    🏷️ {p.name} &minus;{formatCents(p.discount_cents)}
                  </p>
                ))}
                <p className="text-gray-500 text-xs mt-1">
                  Avant remise : {formatCents(order.totalCents)}
                </p>
              </div>
            )}
          </div>

          {/* Sélection méthode */}
          <div className="space-y-3">
            <p className="text-gray-400 text-sm uppercase tracking-wider">Moyen de paiement</p>

            {([
              { id: 'card' as Method, label: '💳 Carte bancaire', desc: 'TPE — insérer/présenter la carte' },
              { id: 'cash' as Method, label: '💵 Espèces', desc: 'Saisir le montant remis' },
              { id: 'meal_voucher' as Method, label: '🎟 Ticket restaurant', desc: 'Edenred, Sodexo, Swile…' },
            ] as const).map(({ id, label, desc }) => (
              <button
                key={id}
                onClick={() => order.setPaymentMethod(id)}
                className={[
                  'w-full text-left px-5 py-4 rounded-xl border-2 transition-colors',
                  selectedMethod === id
                    ? 'border-blue-500 bg-blue-950 text-white'
                    : 'border-gray-700 bg-gray-800 text-gray-300 hover:border-gray-600',
                ].join(' ')}
              >
                <div className="font-semibold">{label}</div>
                <div className="text-sm text-gray-500 mt-0.5">{desc}</div>
              </button>
            ))}
          </div>
        </div>

        {/* Colonne droite : saisie espèces ou instructions CB */}
        <div className="w-80">
          {selectedMethod === 'card' && (
            <div className="bg-blue-950 border border-blue-800 rounded-2xl p-6 text-center space-y-4">
              <div className="text-5xl">💳</div>
              <p className="text-blue-200 font-medium">
                Présentez la carte sur le terminal de paiement
              </p>
              <p className="text-blue-400 text-sm">
                Cliquez sur «&nbsp;Paiement reçu&nbsp;» après confirmation du TPE
              </p>
            </div>
          )}

          {selectedMethod === 'meal_voucher' && (
            <div className="bg-orange-950 border border-orange-800 rounded-2xl p-6 text-center space-y-4">
              <div className="text-5xl">🎟</div>
              <p className="text-orange-200 font-medium">
                Acceptez le ticket restaurant via le terminal dédié
              </p>
              <p className="text-orange-400 text-sm">
                Cliquez sur «&nbsp;Paiement reçu&nbsp;» après validation
              </p>
            </div>
          )}

          {selectedMethod === 'cash' && (
            <div className="bg-gray-800 rounded-2xl p-5 space-y-4">
              <p className="text-gray-300 font-medium">Montant remis par le client</p>

              {/* Saisie manuelle */}
              <div className="relative">
                <input
                  type="number"
                  step="0.01"
                  min="0"
                  value={cashInput}
                  onChange={(e) => setCashInput(e.target.value)}
                  placeholder="0,00"
                  className={[
                    'w-full bg-gray-900 border-2 rounded-xl px-4 py-3 text-right text-2xl font-bold',
                    'text-white placeholder-gray-600',
                    'focus:outline-none focus:ring-2 focus:ring-blue-500',
                    cashInsufficient ? 'border-red-600' : 'border-gray-600',
                  ].join(' ')}
                />
                <span className="absolute right-4 top-1/2 -translate-y-1/2 text-gray-400 text-lg">€</span>
              </div>

              {/* Raccourcis montants */}
              <div className="grid grid-cols-2 gap-2">
                {CASH_PRESETS.map((preset) => (
                  <button
                    key={preset}
                    onClick={() => setCashInput((preset / 100).toFixed(2))}
                    className="bg-gray-700 hover:bg-gray-600 text-white rounded-xl py-3 font-semibold transition-colors"
                  >
                    {formatCents(preset)}
                  </button>
                ))}
              </div>

              {/* Rendu monnaie */}
              {cashAmountCents > 0 && !cashInsufficient && (
                <div className="bg-green-950 border border-green-800 rounded-xl px-4 py-3 flex justify-between">
                  <span className="text-green-400">Rendu monnaie</span>
                  <span className="text-green-300 font-bold text-lg">
                    {formatCents(changeCents)}
                  </span>
                </div>
              )}

              {cashInsufficient && (
                <p className="text-red-400 text-sm text-center">
                  Montant insuffisant (manque {formatCents(totalCents - cashAmountCents)})
                </p>
              )}
            </div>
          )}
        </div>
      </main>

      {/* Barre d'actions */}
      <div className="px-6 py-4 bg-gray-900 border-t border-gray-800 flex gap-4">
        <Button
          variant="ghost"
          size="lg"
          onClick={() => navigateTo('order')}
        >
          ← Retour
        </Button>

        <Button
          variant="success"
          size="lg"
          fullWidth
          disabled={!canConfirm}
          loading={order.isConfirmingPayment}
          onClick={handleConfirm}
        >
          Paiement reçu — valider
        </Button>
      </div>

      <StatusBar />
    </div>
  );
}
