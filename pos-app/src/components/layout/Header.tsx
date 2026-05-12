/**
 * Composants de layout : Header et StatusBar.
 */

import React from 'react';
import { useSessionStore, useUiStore } from '@/store';
import { formatCents, formatTimestamp } from '@/api/client';
import { Badge } from '@/components/ui';

// ---------------------------------------------------------------------------
// Header — barre supérieure de la caisse
// ---------------------------------------------------------------------------

interface HeaderProps {
  title: string;
  onZReport?: () => void;
}

export function Header({ title, onZReport }: HeaderProps): React.ReactElement {
  const session = useSessionStore((s) => s.session);
  const navigateTo = useUiStore((s) => s.navigateTo);

  const now = new Date();
  const timeStr = now.toLocaleTimeString('fr-FR', { hour: '2-digit', minute: '2-digit' });
  const dateStr = now.toLocaleDateString('fr-FR', { weekday: 'short', day: '2-digit', month: '2-digit' });

  return (
    <header className="flex items-center justify-between px-6 py-3 bg-gray-900 border-b border-gray-700 shrink-0">
      {/* Titre + session */}
      <div className="flex items-center gap-4">
        <h1 className="text-lg font-bold text-white">{title}</h1>
        {session && (
          <Badge variant="success">
            Session #{session.session_sequence}
          </Badge>
        )}
        {!session && (
          <Badge variant="danger">Aucune session</Badge>
        )}
      </div>

      {/* Horloge */}
      <div className="text-center text-gray-400 text-sm">
        <div className="font-mono text-white text-base">{timeStr}</div>
        <div className="capitalize">{dateStr}</div>
      </div>

      {/* Actions manager */}
      <div className="flex items-center gap-2">
        {session && (
          <button
            onClick={() => navigateTo('cancel')}
            className="px-3 py-2 text-sm text-yellow-400 hover:text-yellow-300 hover:bg-yellow-900/30 rounded-lg transition-colors"
          >
            Annulation
          </button>
        )}
        {session && onZReport && (
          <button
            onClick={onZReport}
            className="px-3 py-2 text-sm text-red-400 hover:text-red-300 hover:bg-red-900/30 rounded-lg transition-colors"
          >
            Rapport Z
          </button>
        )}
      </div>
    </header>
  );
}

// ---------------------------------------------------------------------------
// StatusBar — barre inférieure : totaux de session + erreurs
// ---------------------------------------------------------------------------

export function StatusBar(): React.ReactElement {
  const session = useSessionStore((s) => s.session);
  const globalError = useUiStore((s) => s.globalError);
  const setGlobalError = useUiStore((s) => s.setGlobalError);

  return (
    <footer className="flex items-center justify-between px-6 py-2 bg-gray-950 border-t border-gray-800 shrink-0 min-h-[44px]">
      {/* Erreur globale */}
      {globalError ? (
        <div className="flex items-center gap-2 text-red-400 text-sm flex-1">
          <span className="text-red-500">⚠</span>
          <span className="flex-1 truncate">{globalError}</span>
          <button
            onClick={() => setGlobalError(null)}
            className="text-red-500 hover:text-red-400 shrink-0"
          >
            ✕
          </button>
        </div>
      ) : (
        <>
          {/* Totaux de session */}
          {session ? (
            <div className="flex items-center gap-6 text-sm text-gray-400">
              <span>
                CA : <span className="text-green-400 font-medium">
                  {formatCents(session.totals.sales_cents)}
                </span>
              </span>
              {session.totals.refunds_cents > 0 && (
                <span>
                  Remb. : <span className="text-yellow-400">
                    {formatCents(session.totals.refunds_cents)}
                  </span>
                </span>
              )}
              <span>
                Net : <span className="text-white font-semibold">
                  {formatCents(session.totals.net_revenue_cents)}
                </span>
              </span>
              <span className="text-gray-600">
                {session.totals.entry_count} op.
              </span>
            </div>
          ) : (
            <span className="text-gray-600 text-sm">Aucune session active</span>
          )}

          {/* Session ouverte depuis */}
          {session && (
            <span className="text-gray-600 text-xs">
              Ouverture : {formatTimestamp(session.opened_at_ms)}
            </span>
          )}
        </>
      )}
    </footer>
  );
}
