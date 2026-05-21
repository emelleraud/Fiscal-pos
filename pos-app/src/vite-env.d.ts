/// <reference types="vite/client" />

interface ElectronAPI {
  getApiUrl: () => Promise<string>;
  printText: (text: string) => Promise<{ success: boolean; error?: string }>;
}

declare interface Window {
  electronAPI?: ElectronAPI;
}
