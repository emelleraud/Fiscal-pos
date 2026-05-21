/**
 * TicketScreen — récapitulatif post-paiement (ticket de caisse).
 *
 * Affiche le détail de la transaction, les données fiscales et
 * propose l'impression ainsi que le retour à la prise de commande.
 */

import React from 'react';
import { useOrderStore, useSessionStore, useUiStore } from '@/store';
import { useSession, useOrder } from '@/hooks/useOrder';
import { Header, StatusBar } from '@/components/layout/Header';
import { Button, Badge } from '@/components/ui';
import { formatCents, formatTimestamp } from '@/api/client';

export function TicketScreen(): React.ReactElement {
  const entry = useOrderStore((s) => s.currentFiscalEntry);
  const cart = useOrderStore((s) => s.cart);
  const totalCents = useOrderStore((s) => s.cart.reduce((acc, i) => acc + i.totalCents, 0));
  const paymentMethod = useOrderStore((s) => s.selectedPaymentMethod);
  const amountPaid = useOrderStore((s) => s.amountPaidCents);
  const resetOrder = useOrderStore((s) => s.resetOrder);
  const session = useSessionStore((s) => s.session);
  const navigateTo = useUiStore((s) => s.navigateTo);

  const changeCents = paymentMethod === 'cash' ? amountPaid - totalCents : 0;

  const handlePrint = () => {
    window.print();
  };

  const handleNewOrder = () => {
    resetOrder();
    navigateTo('order');
  };

  const paymentLabel: Record<string, string> = {
    card: 'Carte bancaire',
    cash: 'Espèces',
    meal_voucher: 'Ticket restaurant',
  };

  return (
    <div className="h-screen flex flex-col bg-gray-900 text-white">
      <Header title="Ticket" />

      <main className="flex-1 flex items-start justify-center pt-8 px-6 overflow-y-auto">
        {/* Ticket centré, style reçu */}
        <div className="w-full max-w-sm space-y-6">
          {/* En-tête ticket */}
          <div className="text-center space-y-1">
            <div className="text-green-400 text-4xl">✓</div>
            <h2 className="text-xl font-bold text-white">Paiement accepté</h2>
            <p className="text-gray-400 text-sm">
              {entry ? formatTimestamp(entry.created_at_ms) : ''}
            </p>
          </div>

          {/* Détail des articles */}
          <div className="bg-gray-800 rounded-2xl p-4 space-y-2">
            <h3 className="text-gray-400 text-xs uppercase tracking-wider mb-3">Articles</h3>
            {cart.map(({ menuItem, quantity, totalCents: itemTotal }) => (
              <div key={menuItem.id} className="flex justify-between text-sm">
                <span className="text-gray-300">
                  {quantity}× {menuItem.name}
                </span>
                <span className="text-white font-medium">{formatCents(itemTotal)}</span>
              </div>
            ))}
            <div className="border-t border-gray-700 pt-2 mt-2 flex justify-between font-bold">
              <span className="text-white">Total TTC</span>
              <span className="text-white text-lg">{formatCents(totalCents)}</span>
            </div>
          </div>

          {/* Paiement */}
          <div className="bg-gray-800 rounded-2xl p-4 space-y-2 text-sm">
            <h3 className="text-gray-400 text-xs uppercase tracking-wider mb-3">Paiement</h3>
            <div className="flex justify-between">
              <span className="text-gray-400">Mode</span>
              <span className="text-white">{paymentLabel[paymentMethod ?? 'card'] ?? '—'}</span>
            </div>
            {paymentMethod === 'cash' && (
              <>
                <div className="flex justify-between">
                  <span className="text-gray-400">Remis</span>
                  <span className="text-white">{formatCents(amountPaid)}</span>
                </div>
                <div className="flex justify-between text-green-400">
                  <span>Rendu</span>
                  <span className="font-bold">{formatCents(changeCents)}</span>
                </div>
              </>
            )}
          </div>

          {/* Données fiscales */}
          {entry && (
            <div className="bg-gray-800 rounded-2xl p-4 space-y-2 text-xs">
              <h3 className="text-gray-500 uppercase tracking-wider mb-3">Données fiscales</h3>
              <div className="flex justify-between">
                <span className="text-gray-500">Séquence</span>
                <span className="text-gray-300 font-mono">#{entry.sequence_number}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-gray-500">Entrée ID</span>
                <span className="text-gray-400 font-mono truncate ml-4">{entry.id.slice(0, 8)}…</span>
              </div>
              <div className="flex justify-between">
                <span className="text-gray-500">Hash</span>
                <span className="text-gray-400 font-mono truncate ml-4">{entry.hash_hex.slice(0, 12)}…</span>
              </div>
              <div className="flex justify-between">
                <span className="text-gray-500">TVA {entry.tva_rate}%</span>
                <span className="text-gray-300">{formatCents(entry.tva_cents)}</span>
              </div>
              {session && (
                <div className="flex justify-between">
                  <span className="text-gray-500">Session</span>
                  <Badge variant="success">#{session.session_sequence}</Badge>
                </div>
              )}
            </div>
          )}
        </div>
      </main>

      {/* Actions */}
      <div className="px-6 py-4 bg-gray-900 border-t border-gray-800 flex gap-4">
        <Button variant="ghost" size="lg" onClick={handlePrint}>
          🖨 Imprimer
        </Button>
        <Button variant="primary" size="lg" fullWidth onClick={handleNewOrder}>
          Nouvelle commande
        </Button>
      </div>

      <StatusBar />
    </div>
  );
}

// =============================================================================
// CancelScreen — workflow d'annulation avec motif obligatoire
// =============================================================================

export function CancelScreen(): React.ReactElement {
  const [reason, setReason] = React.useState('');
  const [confirmed, setConfirmed] = React.useState(false);

  const { cancelOrder, isCancelling, currentFiscalEntry } = useOrder();

  const navigateTo = useUiStore((s) => s.navigateTo);

  const PRESET_REASONS = [
    'Erreur de saisie',
    'Article indisponible',
    'Client a changé d\'avis',
    'Problème de caisse',
    'Test / Formation',
  ];

  const canCancel = reason.trim().length >= 3 && confirmed && !!currentFiscalEntry;

  return (
    <div className="h-screen flex flex-col bg-gray-900 text-white">
      <Header title="Annulation" />

      <main className="flex-1 flex items-start justify-center pt-12 px-6">
        <div className="w-full max-w-lg space-y-6">
          {/* Avertissement */}
          <div className="bg-yellow-950 border border-yellow-800 rounded-2xl p-5 text-center space-y-2">
            <div className="text-3xl">⚠️</div>
            <h2 className="text-yellow-200 font-bold text-lg">Annulation de commande</h2>
            <p className="text-yellow-400 text-sm">
              Cette opération est enregistrée dans le journal fiscal (NF525).
              Un motif obligatoire est requis.
            </p>
          </div>

          {/* Commande à annuler */}
          {currentFiscalEntry && (
            <div className="bg-gray-800 rounded-2xl p-4 flex justify-between text-sm">
              <span className="text-gray-400">Montant annulé</span>
              <span className="text-red-400 font-bold text-lg">
                {formatCents(currentFiscalEntry.amount_ttc_cents)}
              </span>
            </div>
          )}

          {!currentFiscalEntry && (
            <div className="bg-gray-800 rounded-2xl p-4 text-center text-gray-500 text-sm">
              Aucune commande récente à annuler sur cet écran.
              <br />
              Retournez à la caisse et sélectionnez la commande concernée.
            </div>
          )}

          {/* Motif */}
          <div className="space-y-3">
            <label className="text-gray-400 text-sm block">Motif de l'annulation</label>
            <div className="flex flex-wrap gap-2">
              {PRESET_REASONS.map((r) => (
                <button
                  key={r}
                  onClick={() => setReason(r)}
                  className={[
                    'px-3 py-2 rounded-xl text-sm border transition-colors',
                    reason === r
                      ? 'border-blue-500 bg-blue-950 text-white'
                      : 'border-gray-700 bg-gray-800 text-gray-400 hover:border-gray-600',
                  ].join(' ')}
                >
                  {r}
                </button>
              ))}
            </div>
            <textarea
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              placeholder="Ou saisissez un motif libre (minimum 3 caractères)…"
              rows={2}
              className="w-full bg-gray-800 border border-gray-700 rounded-xl px-4 py-3 text-white text-sm placeholder-gray-600 focus:outline-none focus:ring-2 focus:ring-blue-500 resize-none"
            />
          </div>

          {/* Confirmation manager */}
          <label className="flex items-center gap-3 cursor-pointer">
            <input
              type="checkbox"
              checked={confirmed}
              onChange={(e) => setConfirmed(e.target.checked)}
              className="w-5 h-5 rounded accent-blue-500"
            />
            <span className="text-gray-300 text-sm">
              Je confirme l'annulation — cette opération est définitive et traçable
            </span>
          </label>
        </div>
      </main>

      {/* Actions */}
      <div className="px-6 py-4 bg-gray-900 border-t border-gray-800 flex gap-4">
        <Button variant="ghost" size="lg" onClick={() => navigateTo('order')}>
          ← Retour
        </Button>
        <Button
          variant="danger"
          size="lg"
          fullWidth
          disabled={!canCancel}
          loading={isCancelling}
          onClick={() => cancelOrder(reason)}
        >
          Confirmer l'annulation
        </Button>
      </div>

      <StatusBar />
    </div>
  );
}

// =============================================================================
// ZReportScreen — rapport Z et clôture de caisse
// =============================================================================

export function ZReportScreen(): React.ReactElement {
  const lastZReport = useUiStore((s) => s.lastZReport);
  const lastZReportText = useUiStore((s) => s.lastZReportText);
  const clearZReport = useUiStore((s) => s.clearZReport);
  const navigateTo = useUiStore((s) => s.navigateTo);
  const { closeSession, isClosing } = useSession();
  const session = useSessionStore((s) => s.session);

  const handlePrint = () => {
    if (lastZReportText) {
      // En production : envoyer via IPC Electron au driver imprimante
      alert(lastZReportText);
    }
  };

  return (
    <div className="h-screen flex flex-col bg-gray-900 text-white">
      <Header title="Rapport Z" />

      <main className="flex-1 overflow-y-auto px-6 py-6">
        {lastZReport ? (
          // Rapport Z généré — affichage des résultats
          <div className="max-w-2xl mx-auto space-y-6">
            {/* En-tête succès */}
            <div className="text-center space-y-2">
              <div className="text-5xl">✅</div>
              <h2 className="text-2xl font-bold text-white">Session clôturée</h2>
              <p className="text-gray-400">
                Session #{lastZReport.session_sequence} — {lastZReport.entry_count} opérations
              </p>
            </div>

            {/* Totaux */}
            <div className="grid grid-cols-2 gap-4">
              {[
                { label: 'CA Brut', value: lastZReport.total_sales_cents, color: 'text-green-400' },
                { label: 'Remboursements', value: lastZReport.total_refunds_cents, color: 'text-yellow-400' },
                { label: 'Annulations', value: lastZReport.total_cancels_cents, color: 'text-orange-400' },
                { label: 'CA Net', value: lastZReport.net_revenue_cents, color: 'text-white', big: true },
              ].map(({ label, value, color, big }) => (
                <div key={label} className="bg-gray-800 rounded-2xl p-4">
                  <p className="text-gray-400 text-sm">{label}</p>
                  <p className={`font-bold mt-1 ${big ? 'text-2xl' : 'text-xl'} ${color}`}>
                    {formatCents(value)}
                  </p>
                </div>
              ))}
            </div>

            {/* Hash de clôture */}
            <div className="bg-gray-800 rounded-2xl p-4 space-y-1 text-sm">
              <p className="text-gray-500 uppercase tracking-wider text-xs">Hash de clôture NF525</p>
              <p className="font-mono text-gray-300 break-all text-xs">
                {lastZReport.closing_hash_hex}
              </p>
              <p className="text-gray-500 text-xs">
                Séquences {lastZReport.first_sequence} → {lastZReport.last_sequence}
              </p>
            </div>

            {/* Ventilation TVA */}
            <div className="bg-gray-800 rounded-2xl p-4 space-y-2 text-sm">
              <p className="text-gray-500 uppercase tracking-wider text-xs">Ventilation TVA (NF525)</p>
              {[
                { label: '5,5%', ht: lastZReport.tva_5_5_ht_cents, tva: lastZReport.tva_5_5_tva_cents, ttc: lastZReport.tva_5_5_ht_cents + lastZReport.tva_5_5_tva_cents },
                { label: '10%',  ht: lastZReport.tva_10_ht_cents,  tva: lastZReport.tva_10_tva_cents,  ttc: lastZReport.tva_10_ht_cents + lastZReport.tva_10_tva_cents },
                { label: '20%',  ht: lastZReport.tva_20_ht_cents,  tva: lastZReport.tva_20_tva_cents,  ttc: lastZReport.tva_20_ht_cents + lastZReport.tva_20_tva_cents },
              ].filter(r => r.ttc > 0).map(({ label, ht, tva, ttc }) => (
                <div key={label} className="flex justify-between text-xs">
                  <span className="text-gray-400 w-10">{label}</span>
                  <span className="text-gray-500">HT {formatCents(ht)}</span>
                  <span className="text-gray-500">TVA {formatCents(tva)}</span>
                  <span className="text-gray-300 font-mono">TTC {formatCents(ttc)}</span>
                </div>
              ))}
            </div>
          </div>
        ) : session ? (
          // Aucun rapport Z — proposition de clôturer
          <div className="max-w-lg mx-auto space-y-6 pt-8 text-center">
            <div className="text-5xl">🔒</div>
            <h2 className="text-2xl font-bold">Clôturer la session</h2>
            <p className="text-gray-400">
              La clôture génère le rapport Z fiscal (NF525 §6).
              Cette opération est irréversible pour la session courante.
            </p>

            {/* Résumé session courante */}
            <div className="bg-gray-800 rounded-2xl p-5 text-left space-y-2 text-sm">
              <div className="flex justify-between">
                <span className="text-gray-400">Session</span>
                <span className="text-white">#{session.session_sequence}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-gray-400">Ventes</span>
                <span className="text-green-400">{formatCents(session.totals.sales_cents)}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-gray-400">CA net</span>
                <span className="text-white font-bold">{formatCents(session.totals.net_revenue_cents)}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-gray-400">Opérations</span>
                <span className="text-white">{session.totals.entry_count}</span>
              </div>
            </div>

            <Button
              variant="danger"
              size="xl"
              fullWidth
              loading={isClosing}
              onClick={() => closeSession()}
            >
              Clôturer et générer le rapport Z
            </Button>
          </div>
        ) : (
          <div className="text-center pt-12 text-gray-500">
            Aucune session active
          </div>
        )}
      </main>

      {/* Actions */}
      <div className="px-6 py-4 bg-gray-900 border-t border-gray-800 flex gap-4">
        <Button
          variant="ghost"
          size="lg"
          onClick={() => {
            clearZReport();
            navigateTo('order');
          }}
        >
          ← Retour caisse
        </Button>
        {lastZReport && (
          <Button variant="secondary" size="lg" onClick={handlePrint}>
            🖨 Imprimer rapport Z
          </Button>
        )}
      </div>

      <StatusBar />
    </div>
  );
}
