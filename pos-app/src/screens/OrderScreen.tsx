/**
 * OrderScreen — écran principal de prise de commande.
 *
 * Layout : grille de produits (gauche) + panier récapitulatif (droite).
 * Navigation : valider le panier → PaymentScreen.
 *
 * Règle : aucun appel API direct ici — tout passe par useOrder().
 * Aucune logique fiscale — le montant est simplement la somme du panier.
 */

import React, { useState } from 'react';
import { useMenu, useOrder } from '@/hooks/useOrder';
import { useUiStore } from '@/store';
import { Header, StatusBar } from '@/components/layout/Header';
import { Button, Card, Spinner, AlertBanner, Badge } from '@/components/ui';
import { formatCents } from '@/api/client';
import type { MenuItem } from '@/api/client';

// ---------------------------------------------------------------------------
// Grille des produits
// ---------------------------------------------------------------------------

function ProductGrid({
  items,
  onAdd,
}: {
  items: MenuItem[];
  onAdd: (item: MenuItem) => void;
}): React.ReactElement {
  const [activeCategory, setActiveCategory] = useState<string>('all');

  const categories = ['all', ...new Set(items.map((i) => i.category))];
  const filtered =
    activeCategory === 'all'
      ? items.filter((i) => i.available)
      : items.filter((i) => i.category === activeCategory && i.available);

  return (
    <div className="flex flex-col gap-4 flex-1 overflow-hidden">
      {/* Filtres par catégorie */}
      <div className="flex gap-2 overflow-x-auto pb-1 shrink-0">
        {categories.map((cat) => (
          <button
            key={cat}
            onClick={() => setActiveCategory(cat)}
            className={[
              'px-4 py-2 rounded-xl text-sm font-medium whitespace-nowrap transition-colors',
              activeCategory === cat
                ? 'bg-blue-600 text-white'
                : 'bg-gray-800 text-gray-400 hover:bg-gray-700 hover:text-white',
            ].join(' ')}
          >
            {cat === 'all' ? 'Tout' : cat}
          </button>
        ))}
      </div>

      {/* Grille tactile */}
      <div className="grid grid-cols-3 gap-3 overflow-y-auto flex-1 content-start">
        {filtered.map((item) => (
          <Card
            key={item.id}
            onClick={() => onAdd(item)}
            className="p-4 flex flex-col gap-2 select-none"
          >
            <div className="text-white font-semibold text-sm leading-tight">
              {item.name}
            </div>
            <div className="flex items-center justify-between mt-auto pt-2">
              <span className="text-blue-400 font-bold text-base">
                {formatCents(item.price_ttc_cents)}
              </span>
              <Badge variant="default" className="text-xs">
                TVA {item.tva_rate === 'reduit5_5' ? '5,5%' : item.tva_rate === 'intermediaire10' ? '10%' : '20%'}
              </Badge>
            </div>
          </Card>
        ))}
        {filtered.length === 0 && (
          <div className="col-span-3 text-center text-gray-600 py-12">
            Aucun article dans cette catégorie
          </div>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Panier
// ---------------------------------------------------------------------------

function CartPanel({
  order,
}: {
  order: ReturnType<typeof useOrder>;
}): React.ReactElement {
  const isEmpty = order.cart.length === 0;

  return (
    <aside className="w-80 flex flex-col gap-4 border-l border-gray-700 pl-4">
      <h2 className="text-white font-bold text-lg shrink-0">Commande</h2>

      {/* Liste des articles */}
      <div className="flex-1 overflow-y-auto space-y-2">
        {isEmpty ? (
          <p className="text-gray-500 text-sm text-center pt-8">
            Appuyez sur un article pour l'ajouter
          </p>
        ) : (
          order.cart.map(({ menuItem, quantity, totalCents }) => (
            <div
              key={menuItem.id}
              className="flex items-center gap-3 bg-gray-800 rounded-xl px-3 py-2"
            >
              {/* Quantité */}
              <div className="flex items-center gap-1 shrink-0">
                <button
                  onClick={() => order.setQuantity(menuItem.id, quantity - 1)}
                  className="w-7 h-7 rounded-lg bg-gray-700 hover:bg-gray-600 text-white text-sm flex items-center justify-center"
                >
                  −
                </button>
                <span className="w-6 text-center text-white font-bold text-sm">
                  {quantity}
                </span>
                <button
                  onClick={() => order.addItem(menuItem)}
                  className="w-7 h-7 rounded-lg bg-gray-700 hover:bg-gray-600 text-white text-sm flex items-center justify-center"
                >
                  +
                </button>
              </div>

              {/* Nom */}
              <span className="flex-1 text-white text-sm truncate">
                {menuItem.name}
              </span>

              {/* Prix total */}
              <span className="text-blue-400 font-semibold text-sm shrink-0">
                {formatCents(totalCents)}
              </span>

              {/* Supprimer */}
              <button
                onClick={() => order.removeItem(menuItem.id)}
                className="text-gray-600 hover:text-red-400 transition-colors shrink-0"
              >
                ✕
              </button>
            </div>
          ))
        )}
      </div>

      {/* Total + bouton valider */}
      <div className="shrink-0 space-y-3 pt-2 border-t border-gray-700">
        <div className="flex justify-between items-center">
          <span className="text-gray-400">Total TTC</span>
          <span className="text-white font-bold text-xl">
            {formatCents(order.totalCents)}
          </span>
        </div>

        <Button
          variant="success"
          size="lg"
          fullWidth
          disabled={isEmpty}
          loading={order.isSubmitting}
          onClick={() => order.submitOrder()}
        >
          {isEmpty ? 'Panier vide' : `Encaisser ${formatCents(order.totalCents)}`}
        </Button>

        {!isEmpty && (
          <Button
            variant="ghost"
            size="sm"
            fullWidth
            onClick={() => order.clearCart()}
          >
            Vider le panier
          </Button>
        )}
      </div>
    </aside>
  );
}

// ---------------------------------------------------------------------------
// OrderScreen
// ---------------------------------------------------------------------------

export function OrderScreen(): React.ReactElement {
  const { data: menu, isLoading, isError } = useMenu();
  const order = useOrder();
  const navigateTo = useUiStore((s) => s.navigateTo);
  const globalError = useUiStore((s) => s.globalError);
  const setGlobalError = useUiStore((s) => s.setGlobalError);

  return (
    <div className="h-screen flex flex-col bg-gray-900 text-white">
      <Header
        title="Caisse"
        onZReport={() => navigateTo('z_report')}
      />

      {/* Erreur globale */}
      {globalError && (
        <div className="px-6 pt-4">
          <AlertBanner
            message={globalError}
            type="error"
            onDismiss={() => setGlobalError(null)}
          />
        </div>
      )}

      {/* Contenu principal */}
      <main className="flex-1 flex gap-6 p-6 overflow-hidden">
        {isLoading && (
          <div className="flex-1 flex items-center justify-center">
            <div className="text-center space-y-4">
              <Spinner size="lg" />
              <p className="text-gray-400">Chargement de la carte…</p>
            </div>
          </div>
        )}

        {isError && (
          <div className="flex-1 flex items-center justify-center">
            <AlertBanner
              message="Impossible de charger la carte. Vérifiez la connexion à l'edge-api."
              type="error"
            />
          </div>
        )}

        {menu && !isLoading && (
          <>
            <ProductGrid items={menu.items} onAdd={order.addItem} />
            <CartPanel order={order} />
          </>
        )}
      </main>

      <StatusBar />
    </div>
  );
}
