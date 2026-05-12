/**
 * Hooks custom — logique métier de la caisse.
 *
 * useMenu    : chargement de la carte via React Query
 * useSession : gestion de la session (ouverture, clôture, polling)
 * useOrder   : soumission du panier, paiement, annulation
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  getMenu,
  getCurrentSession,
  openSession,
  closeSession,
  createOrder,
  payOrder,
  cancelOrder,
  generateOrderRef,
  menuTvaRateToApi,
  type MenuItem,
  type PaymentMethod,
} from '@/api/client';
import {
  useSessionStore,
  useOrderStore,
  useUiStore,
  selectCartTotal,
} from '@/store';

// ---------------------------------------------------------------------------
// useMenu
// ---------------------------------------------------------------------------

/**
 * Charge la carte active depuis l'edge-api.
 * Rafraîchit toutes les 5 minutes (la carte change rarement).
 */
export function useMenu() {
  return useQuery({
    queryKey: ['menu'],
    queryFn: getMenu,
    staleTime: 5 * 60 * 1000,  // 5 minutes
    retry: 3,
    retryDelay: (attempt) => Math.min(1000 * 2 ** attempt, 10_000),
  });
}

// ---------------------------------------------------------------------------
// useSession
// ---------------------------------------------------------------------------

/**
 * Gère la session de caisse : chargement, ouverture, clôture.
 *
 * Poll la session active toutes les 30 secondes pour synchroniser
 * les totaux entre plusieurs terminaux (si le restaurant en a plusieurs).
 */
export function useSession() {
  const setSession = useSessionStore((s) => s.setSession);
  const setGlobalError = useUiStore((s) => s.setGlobalError);
  const navigateTo = useUiStore((s) => s.navigateTo);
  const setLastZReport = useUiStore((s) => s.setLastZReport);
  const queryClient = useQueryClient();

  // Chargement initial de la session
  const sessionQuery = useQuery({
    queryKey: ['session', 'current'],
    queryFn: async () => {
      try {
        const session = await getCurrentSession();
        setSession(session);
        return session;
      } catch {
        // 409 NO_ACTIVE_SESSION = pas d'erreur, juste pas de session
        setSession(null);
        return null;
      }
    },
    refetchInterval: 30_000, // Poll toutes les 30 secondes
    retry: false,
  });

  // Ouverture de session
  const openMutation = useMutation({
    mutationFn: openSession,
    onSuccess: (session) => {
      setSession(session);
      queryClient.setQueryData(['session', 'current'], session);
      navigateTo('order');
      setGlobalError(null);
    },
    onError: (err: Error) => {
      setGlobalError(`Impossible d'ouvrir la session : ${err.message}`);
    },
  });

  // Clôture de session
  const closeMutation = useMutation({
    mutationFn: closeSession,
    onSuccess: (response) => {
      setSession(null);
      queryClient.setQueryData(['session', 'current'], null);
      setLastZReport(response.z_report, response.printable_text);
      navigateTo('z_report');
      setGlobalError(null);
    },
    onError: (err: Error) => {
      setGlobalError(`Impossible de clôturer la session : ${err.message}`);
    },
  });

  return {
    session: sessionQuery.data,
    isLoading: sessionQuery.isLoading,
    openSession: openMutation.mutate,
    isOpening: openMutation.isPending,
    closeSession: closeMutation.mutate,
    isClosing: closeMutation.isPending,
  };
}

// ---------------------------------------------------------------------------
// useOrder
// ---------------------------------------------------------------------------

/**
 * Gère le cycle de vie d'une commande :
 * 1. Construction du panier (ajout/suppression/quantité)
 * 2. Soumission fiscale (POST /orders)
 * 3. Confirmation du paiement (POST /orders/:id/pay)
 * 4. Annulation (POST /orders/:id/cancel)
 */
export function useOrder() {
  const cart = useOrderStore((s) => s.cart);
  const totalCents = useOrderStore(selectCartTotal);
  const addItem = useOrderStore((s) => s.addItem);
  const removeItem = useOrderStore((s) => s.removeItem);
  const setQuantity = useOrderStore((s) => s.setQuantity);
  const clearCart = useOrderStore((s) => s.clearCart);
  const setCurrentOrder = useOrderStore((s) => s.setCurrentOrder);
  const setPaymentMethod = useOrderStore((s) => s.setPaymentMethod);
  const setAmountPaid = useOrderStore((s) => s.setAmountPaid);
  const resetOrder = useOrderStore((s) => s.resetOrder);
  const currentOrderId = useOrderStore((s) => s.currentOrderId);
  const currentFiscalEntry = useOrderStore((s) => s.currentFiscalEntry);
  const selectedPaymentMethod = useOrderStore((s) => s.selectedPaymentMethod);
  const amountPaidCents = useOrderStore((s) => s.amountPaidCents);

  const session = useSessionStore((s) => s.session);
  const navigateTo = useUiStore((s) => s.navigateTo);
  const setGlobalError = useUiStore((s) => s.setGlobalError);

  // Taux de TVA dominant du panier (le plus fréquent)
  const dominantTvaRate = (): '5.5' | '10' | '20' => {
    if (cart.length === 0) return '10';
    const counts = { '5.5': 0, '10': 0, '20': 0 };
    for (const item of cart) {
      const rate = menuTvaRateToApi(item.menuItem.tva_rate);
      counts[rate]++;
    }
    return (Object.entries(counts).sort(([, a], [, b]) => b - a)[0]?.[0] ?? '10') as '5.5' | '10' | '20';
  };

  // Soumission fiscale de la commande (enregistrement dans le journal)
  const submitMutation = useMutation({
    mutationFn: async () => {
      if (!session) throw new Error('Aucune session active');
      if (cart.length === 0) throw new Error('Panier vide');

      return createOrder({
        order_reference: generateOrderRef(),
        amount_ttc_cents: totalCents,
        tva_rate: dominantTvaRate(),
        payment_method: 'card', // Sera confirmé à l'étape paiement
      });
    },
    onSuccess: (response) => {
      setCurrentOrder(response.order_id, response.fiscal_entry);
      navigateTo('payment');
      setGlobalError(null);
    },
    onError: (err: Error) => {
      setGlobalError(`Erreur lors de l'enregistrement fiscal : ${err.message}`);
    },
  });

  // Confirmation du paiement
  const payMutation = useMutation({
    mutationFn: async () => {
      if (!currentOrderId || !selectedPaymentMethod) {
        throw new Error('Commande ou moyen de paiement manquant');
      }
      return payOrder(currentOrderId, {
        payment_method: selectedPaymentMethod as PaymentMethod,
        amount_paid_cents: amountPaidCents || totalCents,
      });
    },
    onSuccess: () => {
      navigateTo('ticket');
      setGlobalError(null);
    },
    onError: (err: Error) => {
      setGlobalError(`Erreur lors du paiement : ${err.message}`);
    },
  });

  // Annulation
  const cancelMutation = useMutation({
    mutationFn: async (reason: string) => {
      if (!currentOrderId || !currentFiscalEntry) {
        throw new Error('Aucune commande à annuler');
      }
      return cancelOrder(currentOrderId, {
        reason,
        fiscal_entry_id: currentFiscalEntry.id,
        amount_ttc_cents: currentFiscalEntry.amount_ttc_cents,
        tva_rate: currentFiscalEntry.tva_rate as '5.5' | '10' | '20',
      });
    },
    onSuccess: () => {
      resetOrder();
      navigateTo('order');
      setGlobalError(null);
    },
    onError: (err: Error) => {
      setGlobalError(`Erreur lors de l'annulation : ${err.message}`);
    },
  });

  return {
    // Panier
    cart,
    totalCents,
    addItem: (item: MenuItem) => addItem(item),
    removeItem: (id: string) => removeItem(id),
    setQuantity: (id: string, qty: number) => setQuantity(id, qty),
    clearCart,

    // Commande courante
    currentOrderId,
    currentFiscalEntry,
    selectedPaymentMethod,
    amountPaidCents,
    setPaymentMethod: (m: 'card' | 'cash' | 'meal_voucher') => setPaymentMethod(m),
    setAmountPaid: (cents: number) => setAmountPaid(cents),

    // Actions
    submitOrder: () => submitMutation.mutate(),
    isSubmitting: submitMutation.isPending,
    confirmPayment: () => payMutation.mutate(),
    isConfirmingPayment: payMutation.isPending,
    cancelOrder: (reason: string) => cancelMutation.mutate(reason),
    isCancelling: cancelMutation.isPending,
    resetOrder,
  };
}
