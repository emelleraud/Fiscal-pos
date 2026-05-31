/**
 * PromoModal — sélection des promotions avant encaissement.
 *
 * Affiche les promotions automatiques (informatives) et les promotions
 * manuelles (sélectionnables par le caissier).
 */

import React, { useEffect, useState } from 'react';
import { getAvailablePromotions, type AvailablePromo } from '@/api/client';
import { useOrderStore } from '@/store';
import { Button } from '@/components/ui';

interface Props {
  onClose: () => void;
}

export function PromoModal({ onClose }: Props): React.ReactElement {
  const [promos, setPromos] = useState<AvailablePromo[]>([]);
  const [loading, setLoading] = useState(true);
  const { selectedPromoIds, setSelectedPromoIds } = useOrderStore();
  const [selected, setSelected] = useState<string[]>(selectedPromoIds);

  useEffect(() => {
    let mounted = true
    getAvailablePromotions()
      .then(data => { if (mounted) setPromos(data) })
      .catch(() => { if (mounted) setPromos([]) })
      .finally(() => { if (mounted) setLoading(false) })
    return () => { mounted = false }
  }, []);

  const autoPromos = promos.filter((p) => p.trigger === 'auto');
  const manualPromos = promos.filter((p) => p.trigger === 'manual');

  const toggle = (id: string) =>
    setSelected((prev) =>
      prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id]
    );

  const confirm = () => {
    setSelectedPromoIds(selected);
    onClose();
  };

  const formatDiscount = (p: AvailablePromo): string => {
    if (p.value_cents !== null) return `-${(p.value_cents / 100).toFixed(2)} €`;
    if (p.value_bps !== null)   return `-${(p.value_bps / 100).toFixed(0)} %`;
    return 'offert';
  };

  return (
    <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50">
      <div className="bg-gray-800 rounded-2xl p-6 w-80 max-h-[80vh] overflow-y-auto space-y-4">
        <h3 className="text-white font-bold text-lg">Promotions disponibles</h3>

        {loading && (
          <p className="text-gray-400 text-sm text-center">Chargement…</p>
        )}

        {!loading && autoPromos.length > 0 && (
          <div>
            <p className="text-gray-500 text-xs uppercase tracking-wider mb-2">
              Appliquées automatiquement
            </p>
            {autoPromos.map((p) => (
              <div
                key={p.id}
                className="flex justify-between items-center px-3 py-2 bg-green-950 rounded-xl mb-1"
              >
                <span className="text-white text-sm">{p.name}</span>
                <span className="text-green-400 font-semibold text-sm">
                  {formatDiscount(p)}
                </span>
              </div>
            ))}
          </div>
        )}

        {!loading && manualPromos.length > 0 && (
          <div>
            <p className="text-gray-500 text-xs uppercase tracking-wider mb-2">
              Sélection manuelle
            </p>
            {manualPromos.map((p) => (
              <label
                key={p.id}
                className={[
                  'flex justify-between items-center px-3 py-2 rounded-xl mb-1 cursor-pointer border transition-colors',
                  selected.includes(p.id)
                    ? 'bg-blue-950 border-blue-500'
                    : 'bg-gray-700 border-transparent hover:border-gray-500',
                ].join(' ')}
              >
                <span className="flex items-center gap-2">
                  <input
                    type="checkbox"
                    className="accent-blue-500"
                    checked={selected.includes(p.id)}
                    onChange={() => toggle(p.id)}
                  />
                  <span className="text-white text-sm">{p.name}</span>
                </span>
                <span className="text-blue-400 font-semibold text-sm">
                  {formatDiscount(p)}
                </span>
              </label>
            ))}
          </div>
        )}

        {!loading && promos.length === 0 && (
          <p className="text-gray-500 text-sm text-center">
            Aucune promotion disponible
          </p>
        )}

        <div className="flex gap-2 pt-2">
          <Button variant="primary" size="lg" fullWidth onClick={confirm}>
            Confirmer
          </Button>
          <Button variant="ghost" size="lg" onClick={onClose}>
            Fermer
          </Button>
        </div>
      </div>
    </div>
  );
}
