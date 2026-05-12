/**
 * Point d'entrée React — montage de l'application et routing par écran.
 *
 * Navigation : state machine via Zustand `useUiStore.currentScreen`.
 * Pas de react-router : l'application est un SPA mono-page sans URL significatives.
 * La navigation est entièrement pilotée par l'état Zustand.
 */

import React, { useEffect } from 'react';
import ReactDOM from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { getCurrentSession, openSession } from '@/api/client';
import { useSessionStore, useUiStore } from '@/store';
import { OrderScreen } from '@/screens/OrderScreen';
import { PaymentScreen } from '@/screens/PaymentScreen';
import { TicketScreen, CancelScreen, ZReportScreen } from '@/screens/TicketScreen';
import './index.css';

// ---------------------------------------------------------------------------
// React Query — configuration globale
// ---------------------------------------------------------------------------

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 60_000,           // 1 minute par défaut
      retry: 3,
      retryDelay: (attempt) => Math.min(1000 * 2 ** attempt, 10_000),
    },
    mutations: {
      retry: 0, // Les mutations fiscales ne se relancent pas automatiquement
    },
  },
});

// ---------------------------------------------------------------------------
// Écran de login (ouverture de session)
// ---------------------------------------------------------------------------

function LoginScreen(): React.ReactElement {
  const [isOpening, setIsOpening] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const setSession = useSessionStore((s) => s.setSession);
  const navigateTo = useUiStore((s) => s.navigateTo);

  const handleOpenSession = async () => {
    setIsOpening(true);
    setError(null);
    try {
      const session = await openSession();
      setSession(session);
      navigateTo('order');
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Erreur inconnue');
    } finally {
      setIsOpening(false);
    }
  };

  return (
    <div className="h-screen bg-gray-900 flex flex-col items-center justify-center gap-8">
      <div className="text-center space-y-2">
        <div className="text-6xl">🏪</div>
        <h1 className="text-3xl font-bold text-white">Caisse POS</h1>
        <p className="text-gray-400">Système de caisse certifiable NF525</p>
      </div>

      {error && (
        <div className="bg-red-900/50 border border-red-700 text-red-300 rounded-xl px-6 py-3 text-sm max-w-sm text-center">
          {error}
        </div>
      )}

      <button
        onClick={() => void handleOpenSession()}
        disabled={isOpening}
        className="px-12 py-5 bg-blue-600 hover:bg-blue-500 disabled:opacity-50 text-white text-xl font-bold rounded-2xl transition-colors"
      >
        {isOpening ? 'Ouverture en cours…' : 'Ouvrir la caisse'}
      </button>

      <p className="text-gray-600 text-xs text-center max-w-xs">
        L'ouverture de session démarre le journal fiscal NF525.
        Toutes les opérations seront enregistrées et chaînées cryptographiquement.
      </p>
    </div>
  );
}

// ---------------------------------------------------------------------------
// App — routing par état Zustand
// ---------------------------------------------------------------------------

function App(): React.ReactElement {
  const currentScreen = useUiStore((s) => s.currentScreen);
  const setSession = useSessionStore((s) => s.setSession);
  const navigateTo = useUiStore((s) => s.navigateTo);

  // Au démarrage : vérifier si une session est déjà active
  useEffect(() => {
    getCurrentSession()
      .then((session) => {
        setSession(session);
        navigateTo('order');
      })
      .catch(() => {
        // 409 = pas de session active, on reste sur login
        navigateTo('login');
      });
  }, [setSession, navigateTo]);

  // Router basé sur l'état
  switch (currentScreen) {
    case 'login':     return <LoginScreen />;
    case 'order':     return <OrderScreen />;
    case 'payment':   return <PaymentScreen />;
    case 'ticket':    return <TicketScreen />;
    case 'cancel':    return <CancelScreen />;
    case 'z_report':  return <ZReportScreen />;
    default:          return <LoginScreen />;
  }
}

// ---------------------------------------------------------------------------
// Montage React
// ---------------------------------------------------------------------------

const root = document.getElementById('root');
if (!root) throw new Error('Élément #root introuvable dans le HTML');

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </React.StrictMode>
);
