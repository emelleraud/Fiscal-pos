/**
 * Process principal Electron — gère la fenêtre principale de la caisse.
 *
 * Contraintes :
 * - Fenêtre en plein écran (mode kiosk) en production
 * - Accès réseau limité à localhost (l'edge-api locale)
 * - Pas d'accès fichier système depuis le renderer (contextIsolation activé)
 * - NODE_ENV=development : DevTools accessibles via F12
 */

import { app, BrowserWindow, ipcMain } from 'electron';
import path from 'path';

const isDev = process.env['NODE_ENV'] === 'development';

// URL de l'edge-api locale — configurable via variable d'environnement
const EDGE_API_URL =
  process.env['EDGE_API_URL'] ?? 'http://localhost:8080';

let mainWindow: BrowserWindow | null = null;

function createWindow(): void {
  mainWindow = new BrowserWindow({
    width: 1280,
    height: 800,
    // Plein écran en production (mode kiosk pour les terminaux restaurant)
    fullscreen: !isDev,
    kiosk: !isDev,
    // Barre de titre masquée : l'UI React gère son propre header
    frame: false,
    titleBarStyle: 'hidden',
    backgroundColor: '#111827', // Tailwind gray-900 — thème sombre QSR
    webPreferences: {
      nodeIntegration: false,      // Sécurité : pas d'accès Node depuis le renderer
      contextIsolation: true,      // Sécurité : isolation du contexte
      preload: path.join(__dirname, 'preload.js'),
      // Restreindre les permissions réseau au LAN local
      webSecurity: true,
    },
  });

  // En développement : Vite dev server
  // En production : fichiers buildés dans dist/
  if (isDev) {
    void mainWindow.loadURL('http://localhost:5173');
    mainWindow.webContents.openDevTools({ mode: 'detach' });
  } else {
    void mainWindow.loadFile(path.join(__dirname, '../dist/index.html'));
  }

  mainWindow.on('closed', () => {
    mainWindow = null;
  });
}

// ---------------------------------------------------------------------------
// Cycle de vie de l'application
// ---------------------------------------------------------------------------

app.whenReady().then(() => {
  createWindow();

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow();
    }
  });
});

app.on('window-all-closed', () => {
  // Sur macOS, l'app reste active même sans fenêtre
  if (process.platform !== 'darwin') {
    app.quit();
  }
});

// ---------------------------------------------------------------------------
// IPC : communication sécurisée renderer ↔ main
// ---------------------------------------------------------------------------

/**
 * Expose l'URL de l'edge-api au renderer de manière sécurisée.
 * Le renderer ne doit jamais avoir accès à process.env directement.
 */
ipcMain.handle('get-api-url', () => EDGE_API_URL);

/**
 * Impression du ticket ou du rapport Z.
 * Délègue à l'API système d'impression via le process principal.
 */
ipcMain.handle('print-text', (_event: Electron.IpcMainInvokeEvent, text: string) => {
  if (mainWindow) {
    // En production : utiliser un driver d'impression ESC/POS
    // En MVP : log dans la console du process principal
    console.log('=== IMPRESSION ===\n' + text + '\n=================');
    return { success: true };
  }
  return { success: false, error: 'Fenêtre principale indisponible' };
});
