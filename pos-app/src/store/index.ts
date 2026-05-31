/**
 * Store Zustand — état global de la caisse.
 *
 * Trois domaines distincts :
 * - `useSessionStore` — session de caisse active
 * - `useOrderStore`   — commande en cours (panier)
 * - `useUiStore`      — navigation entre écrans, états UI
 *
 * Règle : aucune logique métier dans ce store.
 * Le store ne fait que stocker et exposer l'état.
 * La logique vit dans les hooks custom (useOrder, useSession).
 */

import { create } from 'zustand';
import type { SessionResponse, FiscalEntryResponse, ZReportSummary, MenuItem, AppliedPromo } from '@/api/client';

// ---------------------------------------------------------------------------
// Types internes
// ---------------------------------------------------------------------------

/** Article dans le panier, avec quantité */
export interface CartItem {
  menuItem: MenuItem;
  quantity: number;
  /** Montant TTC total pour cet article (price * quantity) */
  totalCents: number;
}

/** Écrans de l'application */
export type Screen =
  | 'order'     // Grille produits + panier
  | 'payment'   // Sélection moyen de paiement
  | 'ticket'    // Récapitulatif post-paiement
  | 'cancel'    // Workflow annulation
  | 'z_report'  // Clôture de caisse
  | 'login';    // Ouverture de session (si aucune session active)

// ---------------------------------------------------------------------------
// Store session
// ---------------------------------------------------------------------------

interface SessionState {
  session: SessionResponse | null;
  setSession: (s: SessionResponse | null) => void;
  updateTotals: (totals: SessionResponse['totals']) => void;
}

export const useSessionStore = create<SessionState>((set) => ({
  session: null,
  setSession: (s) => set({ session: s }),
  updateTotals: (totals) =>
    set((state) =>
      state.session ? { session: { ...state.session, totals } } : state
    ),
}));

// ---------------------------------------------------------------------------
// Store commande (panier)
// ---------------------------------------------------------------------------

interface OrderState {
  /** Articles dans le panier */
  cart: CartItem[];
  /** Identifiant de la commande courante (après soumission à l'API) */
  currentOrderId: string | null;
  /** Entrée fiscale créée pour la commande courante */
  currentFiscalEntry: FiscalEntryResponse | null;
  /** Moyen de paiement sélectionné */
  selectedPaymentMethod: 'card' | 'cash' | 'meal_voucher' | null;
  /** Montant encaissé en espèces (pour le calcul du rendu monnaie) */
  amountPaidCents: number;

  /** IDs des promotions manuelles sélectionnées */
  selectedPromoIds: string[];
  /** Promotions effectivement appliquées par l'API */
  appliedPromos: AppliedPromo[];

  /** Actions */
  addItem: (item: MenuItem) => void;
  removeItem: (itemId: string) => void;
  setQuantity: (itemId: string, qty: number) => void;
  clearCart: () => void;
  setCurrentOrder: (orderId: string, entry: FiscalEntryResponse) => void;
  setPaymentMethod: (method: 'card' | 'cash' | 'meal_voucher') => void;
  setAmountPaid: (cents: number) => void;
  setSelectedPromoIds: (ids: string[]) => void;
  setAppliedPromos: (promos: AppliedPromo[]) => void;
  resetOrder: () => void;
}

export const useOrderStore = create<OrderState>((set) => ({
  cart: [],
  currentOrderId: null,
  currentFiscalEntry: null,
  selectedPaymentMethod: null,
  amountPaidCents: 0,
  selectedPromoIds: [],
  appliedPromos: [],

  addItem: (item) =>
    set((state) => {
      const existing = state.cart.find((c) => c.menuItem.id === item.id);
      if (existing) {
        return {
          cart: state.cart.map((c) =>
            c.menuItem.id === item.id
              ? {
                  ...c,
                  quantity: c.quantity + 1,
                  totalCents: c.totalCents + item.price_ttc_cents,
                }
              : c
          ),
        };
      }
      return {
        cart: [
          ...state.cart,
          { menuItem: item, quantity: 1, totalCents: item.price_ttc_cents },
        ],
      };
    }),

  removeItem: (itemId) =>
    set((state) => ({
      cart: state.cart.filter((c) => c.menuItem.id !== itemId),
    })),

  setQuantity: (itemId, qty) =>
    set((state) => ({
      cart:
        qty <= 0
          ? state.cart.filter((c) => c.menuItem.id !== itemId)
          : state.cart.map((c) =>
              c.menuItem.id === itemId
                ? { ...c, quantity: qty, totalCents: c.menuItem.price_ttc_cents * qty }
                : c
            ),
    })),

  clearCart: () => set({ cart: [] }),

  setCurrentOrder: (orderId, entry) =>
    set({ currentOrderId: orderId, currentFiscalEntry: entry }),

  setPaymentMethod: (method) => set({ selectedPaymentMethod: method }),

  setAmountPaid: (cents) => set({ amountPaidCents: cents }),

  setSelectedPromoIds: (ids) => set({ selectedPromoIds: ids }),

  setAppliedPromos: (promos) => set({ appliedPromos: promos }),

  resetOrder: () =>
    set({
      cart: [],
      currentOrderId: null,
      currentFiscalEntry: null,
      selectedPaymentMethod: null,
      amountPaidCents: 0,
      selectedPromoIds: [],
      appliedPromos: [],
    }),
}));

/** Sélecteur : montant TTC total du panier en centimes */
export const selectCartTotal = (state: OrderState): number =>
  state.cart.reduce((sum, item) => sum + item.totalCents, 0);

/** Sélecteur : nombre total d'articles dans le panier */
export const selectCartCount = (state: OrderState): number =>
  state.cart.reduce((sum, item) => sum + item.quantity, 0);

// ---------------------------------------------------------------------------
// Store UI
// ---------------------------------------------------------------------------

interface UiState {
  /** Écran actif */
  currentScreen: Screen;
  /** Message d'erreur global (affiché dans la StatusBar) */
  globalError: string | null;
  /** Rapport Z du dernier rapport (pour l'affichage post-clôture) */
  lastZReport: ZReportSummary | null;
  /** Texte imprimable du dernier rapport Z */
  lastZReportText: string | null;

  navigateTo: (screen: Screen) => void;
  setGlobalError: (msg: string | null) => void;
  setLastZReport: (report: ZReportSummary, text: string) => void;
  clearZReport: () => void;
}

export const useUiStore = create<UiState>((set) => ({
  currentScreen: 'login',
  globalError: null,
  lastZReport: null,
  lastZReportText: null,

  navigateTo: (screen) => set({ currentScreen: screen, globalError: null }),
  setGlobalError: (msg) => set({ globalError: msg }),
  setLastZReport: (report, text) =>
    set({ lastZReport: report, lastZReportText: text }),
  clearZReport: () => set({ lastZReport: null, lastZReportText: null }),
}));
