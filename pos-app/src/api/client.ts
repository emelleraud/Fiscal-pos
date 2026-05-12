/**
 * Client HTTP vers l'edge-api locale.
 *
 * Toutes les interactions réseau de l'application passent par ce module.
 * Pas de logique métier ici — uniquement la couche transport.
 *
 * L'URL de base est lue depuis la variable d'env VITE_API_URL
 * (configurée dans .env.local ou injectée par Electron via IPC).
 */

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

const API_BASE = import.meta.env['VITE_API_URL'] ?? 'http://localhost:8080';

// ---------------------------------------------------------------------------
// Types — miroir exact des DTOs de l'edge-api
// ---------------------------------------------------------------------------

export type TvaRate = '5.5' | '10' | '20';
export type PaymentMethod = 'card' | 'cash' | 'meal_voucher';
export type OperationType = 'VENTE' | 'REMBOURSEMENT' | 'ANNULATION' | 'REMISE' | 'CLOTURE_Z';

export interface MenuItem {
  id: string;
  name: string;
  price_ttc_cents: number;
  tva_rate: 'reduit5_5' | 'intermediaire10' | 'normal20';
  category: string;
  available: boolean;
}

export interface MenuResponse {
  items: MenuItem[];
  last_updated_ms: number;
  version: number;
}

export interface SessionTotals {
  sales_cents: number;
  refunds_cents: number;
  cancels_cents: number;
  discounts_cents: number;
  net_revenue_cents: number;
  entry_count: number;
}

export interface SessionResponse {
  session_id: string;
  session_sequence: number;
  status: 'open' | 'closed';
  opened_at_ms: number;
  totals: SessionTotals;
}

export interface FiscalEntryResponse {
  id: string;
  sequence_number: number;
  operation_type: OperationType;
  amount_ttc_cents: number;
  ht_cents: number;
  tva_cents: number;
  tva_rate: string;
  hash_hex: string;
  created_at_ms: number;
}

export interface OrderResponse {
  order_id: string;
  fiscal_entry: FiscalEntryResponse;
}

export interface ZReportSummary {
  id: string;
  session_id: string;
  session_sequence: number;
  generated_at_ms: number;
  total_sales_cents: number;
  total_refunds_cents: number;
  total_cancels_cents: number;
  total_discounts_cents: number;
  net_revenue_cents: number;
  closing_hash_hex: string;
  first_sequence: number;
  last_sequence: number;
  entry_count: number;
}

export interface CloseSessionResponse {
  message: string;
  z_report: ZReportSummary;
  printable_text: string;
}

export interface HealthResponse {
  status: 'ok' | 'degraded';
  version: string;
  database: string;
  active_session: { session_id: string; session_sequence: number; entry_count: number } | null;
}

export interface ApiErrorResponse {
  code: string;
  message: string;
  detail?: unknown;
}

// ---------------------------------------------------------------------------
// Erreur API typée
// ---------------------------------------------------------------------------

export class ApiError extends Error {
  constructor(
    public readonly code: string,
    message: string,
    public readonly httpStatus: number,
    public readonly detail?: unknown
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

// ---------------------------------------------------------------------------
// Fetch générique
// ---------------------------------------------------------------------------

async function apiFetch<T>(
  path: string,
  options: RequestInit = {}
): Promise<T> {
  const url = `${API_BASE}${path}`;
  const response = await fetch(url, {
    ...options,
    headers: {
      'Content-Type': 'application/json',
      ...options.headers,
    },
  });

  if (!response.ok) {
    let errorBody: ApiErrorResponse = {
      code: 'UNKNOWN_ERROR',
      message: `HTTP ${response.status}`,
    };
    try {
      errorBody = (await response.json()) as ApiErrorResponse;
    } catch {
      // Corps non-JSON — on garde le message par défaut
    }
    throw new ApiError(errorBody.code, errorBody.message, response.status, errorBody.detail);
  }

  if (response.status === 204) {
    return undefined as unknown as T;
  }

  return response.json() as Promise<T>;
}

// ---------------------------------------------------------------------------
// API Health
// ---------------------------------------------------------------------------

export const getHealth = (): Promise<HealthResponse> =>
  apiFetch<HealthResponse>('/api/v1/health');

// ---------------------------------------------------------------------------
// API Menu
// ---------------------------------------------------------------------------

export const getMenu = (): Promise<MenuResponse> =>
  apiFetch<MenuResponse>('/api/v1/menu');

// ---------------------------------------------------------------------------
// API Sessions
// ---------------------------------------------------------------------------

export const getCurrentSession = (): Promise<SessionResponse> =>
  apiFetch<SessionResponse>('/api/v1/sessions/current');

export const openSession = (): Promise<SessionResponse> =>
  apiFetch<SessionResponse>('/api/v1/sessions/open', { method: 'POST' });

export const closeSession = (): Promise<CloseSessionResponse> =>
  apiFetch<CloseSessionResponse>('/api/v1/sessions/close', { method: 'POST' });

// ---------------------------------------------------------------------------
// API Commandes
// ---------------------------------------------------------------------------

export interface CreateOrderPayload {
  order_reference: string;
  amount_ttc_cents: number;
  tva_rate: TvaRate;
  payment_method: PaymentMethod;
}

export const createOrder = (payload: CreateOrderPayload): Promise<OrderResponse> =>
  apiFetch<OrderResponse>('/api/v1/orders', {
    method: 'POST',
    body: JSON.stringify(payload),
  });

export interface PayOrderPayload {
  payment_method: PaymentMethod;
  amount_paid_cents: number;
}

export const payOrder = (orderId: string, payload: PayOrderPayload): Promise<OrderResponse> =>
  apiFetch<OrderResponse>(`/api/v1/orders/${orderId}/pay`, {
    method: 'POST',
    body: JSON.stringify(payload),
  });

export interface CancelOrderPayload {
  reason: string;
  fiscal_entry_id: string;
  amount_ttc_cents: number;
  tva_rate: TvaRate;
}

export const cancelOrder = (orderId: string, payload: CancelOrderPayload): Promise<OrderResponse> =>
  apiFetch<OrderResponse>(`/api/v1/orders/${orderId}/cancel`, {
    method: 'POST',
    body: JSON.stringify(payload),
  });

// ---------------------------------------------------------------------------
// Utilitaires
// ---------------------------------------------------------------------------

/** Formate un montant en centimes en euros lisibles (ex: 1099 → "10,99 €") */
export function formatCents(cents: number): string {
  return new Intl.NumberFormat('fr-FR', {
    style: 'currency',
    currency: 'EUR',
  }).format(cents / 100);
}

/** Formate un timestamp Unix ms en date/heure locale française */
export function formatTimestamp(ms: number): string {
  return new Intl.DateTimeFormat('fr-FR', {
    dateStyle: 'short',
    timeStyle: 'medium',
  }).format(new Date(ms));
}

/** Génère une référence de commande unique */
export function generateOrderRef(): string {
  const now = new Date();
  const pad = (n: number, len = 2): string => String(n).padStart(len, '0');
  const date = `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}`;
  const time = `${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`;
  const rand = Math.floor(Math.random() * 1000).toString().padStart(3, '0');
  return `ORD-${date}-${time}-${rand}`;
}

/** Convertit le taux de TVA du menu en string pour l'API */
export function menuTvaRateToApi(rate: MenuItem['tva_rate']): TvaRate {
  switch (rate) {
    case 'reduit5_5': return '5.5';
    case 'intermediaire10': return '10';
    case 'normal20': return '20';
  }
}
