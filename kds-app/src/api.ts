const BASE = import.meta.env.VITE_EDGE_API_URL as string

async function post(path: string, body: unknown): Promise<void> {
  const res = await fetch(`${BASE}${path}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`${path} → HTTP ${res.status}`)
}

async function put(path: string, body: unknown): Promise<void> {
  const res = await fetch(`${BASE}${path}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`${path} → HTTP ${res.status}`)
}

/** Acknowledges toutes les lignes d'une commande pour une station (bump en-tête). */
export async function ackOrder(orderId: string, stationId: string): Promise<void> {
  await post(`/api/v1/kds/orders/${orderId}/ack`, { station_id: stationId })
}

/** Acknowledges une ligne spécifique. */
export async function ackLine(orderId: string, stationId: string, lineId: string): Promise<void> {
  await post(`/api/v1/kds/orders/${orderId}/ack`, { station_id: stationId, line_id: lineId })
}

/** Marque la commande comme servie (2e bump expo, retire de l'ORB). */
export async function markServed(orderId: string, stationId: string): Promise<void> {
  await post(`/api/v1/kds/orders/${orderId}/served`, { station_id: stationId })
}

/** Récupère la configuration KDS locale (profil actif). */
export async function getConfig(): Promise<{ active_profile: string }> {
  const res = await fetch(`${BASE}/api/v1/kds/config`)
  if (!res.ok) throw new Error(`getConfig → HTTP ${res.status}`)
  return res.json() as Promise<{ active_profile: string }>
}

/** Met à jour le profil actif. */
export async function setProfile(profileId: string): Promise<void> {
  await put('/api/v1/kds/config', { active_profile: profileId })
}

/** Récupère les stations actives pour le profil courant. */
export async function getStations(): Promise<import('./types').Station[]> {
  const res = await fetch(`${BASE}/api/v1/kds/stations`)
  if (!res.ok) throw new Error(`getStations → HTTP ${res.status}`)
  return res.json() as Promise<import('./types').Station[]>
}
