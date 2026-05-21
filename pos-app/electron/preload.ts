/**
 * Preload Electron — pont sécurisé entre le process principal et le renderer.
 *
 * Exposé via `contextBridge` avec `contextIsolation: true` :
 * le renderer accède uniquement aux méthodes listées ici, sans accès Node.js direct.
 */

import { contextBridge, ipcRenderer } from 'electron';

contextBridge.exposeInMainWorld('electronAPI', {
  /** Récupère l'URL de l'edge-api configurée dans le process principal. */
  getApiUrl: (): Promise<string> =>
    ipcRenderer.invoke('get-api-url'),

  /** Envoie un texte à imprimer (ticket ou rapport Z) au process principal. */
  printText: (text: string): Promise<{ success: boolean; error?: string }> =>
    ipcRenderer.invoke('print-text', text),
});
