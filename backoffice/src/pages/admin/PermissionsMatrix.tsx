import { useState } from 'react'
import { supabase } from '../../supabaseClient'
import { useNetwork, type NetworkPermission } from '../../context/NetworkContext'
import { useAuth } from '../../context/AuthContext'

const DIMENSIONS = ['menu', 'prices', 'promotions', 'discounts', 'user_management', 'z_reports'] as const
const TARGET_ROLES = ['manager', 'director', 'regional_director'] as const
const DIM_LABELS: Record<string, string> = {
  menu: 'Carte / menu',
  prices: 'Prix TTC',
  promotions: 'Promotions',
  discounts: 'Remises caisse',
  user_management: 'Utilisateurs',
  z_reports: 'Rapports Z',
}

type Scope = 'network' | 'group' | 'site'

interface CellEdit {
  dimension: string
  target_role: string
  locked: boolean
  lock_from: string
  lock_until: string
  reason: string
}

function ruleForScope(
  permissions: NetworkPermission[],
  scope: Scope,
  scopeId: string | null,
  dimension: string,
  targetRole: string
): NetworkPermission | null {
  return (
    permissions.find(
      (p) =>
        p.dimension === dimension &&
        p.target_role === targetRole &&
        (scope === 'network'
          ? p.site_id === null && p.group_id === null
          : scope === 'group'
          ? p.group_id === scopeId && p.site_id === null
          : p.site_id === scopeId && p.group_id === null)
    ) ?? null
  )
}

export default function PermissionsMatrix() {
  const { allSites, groups, permissions, reload } = useNetwork()
  const { role } = useAuth()

  const [scope, setScope] = useState<Scope>('network')
  const [scopeId, setScopeId] = useState<string | null>(null)
  const [editing, setEditing] = useState<CellEdit | null>(null)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const canEditNetwork = role === 'pos_admin'

  function openCell(dimension: string, targetRole: string) {
    const rule = ruleForScope(permissions, scope, scopeId, dimension, targetRole)
    setEditing({
      dimension,
      target_role: targetRole,
      locked: rule?.locked ?? false,
      lock_from: rule?.lock_from ?? '',
      lock_until: rule?.lock_until ?? '',
      reason: rule?.reason ?? '',
    })
    setError(null)
  }

  async function saveCell() {
    if (!editing) return
    if (editing.lock_from && !editing.lock_until) { setError('Heure de fin requise'); return }
    if (!editing.lock_from && editing.lock_until) { setError('Heure de début requise'); return }
    if (editing.lock_from && editing.lock_until && editing.lock_until <= editing.lock_from) {
      setError("L'heure de fin doit être après l'heure de début")
      return
    }
    setSaving(true); setError(null)
    try {
      const existing = ruleForScope(permissions, scope, scopeId, editing.dimension, editing.target_role)
      const payload = {
        dimension: editing.dimension,
        target_role: editing.target_role,
        locked: editing.locked,
        lock_from: editing.lock_from || null,
        lock_until: editing.lock_until || null,
        reason: editing.reason || null,
        site_id: scope === 'site' ? scopeId : null,
        group_id: scope === 'group' ? scopeId : null,
        updated_at: new Date().toISOString(),
      }
      if (existing) {
        const { error: e } = await supabase.from('network_permissions').update(payload).eq('id', existing.id)
        if (e) throw e
      } else {
        const { error: e } = await supabase.from('network_permissions').insert(payload)
        if (e) throw e
      }
      reload()
      setEditing(null)
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Erreur')
    } finally { setSaving(false) }
  }

  return (
    <div style={{ padding: '1.5rem' }}>
      <h2 style={{ marginTop: 0 }}>Matrice de permissions</h2>

      {/* Sélecteur de scope */}
      <div style={{ display: 'flex', gap: 12, alignItems: 'center', marginBottom: '1.5rem', flexWrap: 'wrap' }}>
        {(['network', 'group', 'site'] as Scope[]).map((s) => (
          <label key={s} style={{ display: 'flex', alignItems: 'center', gap: 6, cursor: 'pointer' }}>
            <input
              type="radio"
              checked={scope === s}
              onChange={() => { setScope(s); setScopeId(null); setEditing(null) }}
              disabled={s === 'network' && !canEditNetwork}
            />
            {s === 'network' ? 'Réseau entier' : s === 'group' ? 'Groupe' : 'Site'}
          </label>
        ))}

        {scope === 'group' && (
          <select
            style={{ padding: '0.4rem 0.6rem', border: '1px solid #ddd', borderRadius: 6 }}
            value={scopeId ?? ''}
            onChange={(e) => setScopeId(e.target.value || null)}
          >
            <option value="">— Choisir un groupe —</option>
            {groups.map((g) => <option key={g.id} value={g.id}>{g.name}</option>)}
          </select>
        )}

        {scope === 'site' && (
          <select
            style={{ padding: '0.4rem 0.6rem', border: '1px solid #ddd', borderRadius: 6 }}
            value={scopeId ?? ''}
            onChange={(e) => setScopeId(e.target.value || null)}
          >
            <option value="">— Choisir un site —</option>
            {allSites.map((site) => <option key={site.id} value={site.id}>{site.name} ({site.site_code})</option>)}
          </select>
        )}
      </div>

      {/* Matrice */}
      <table style={{ width: '100%', borderCollapse: 'collapse', marginBottom: '1.5rem' }}>
        <thead>
          <tr style={{ background: '#f5f6fa' }}>
            <th style={{ textAlign: 'left', padding: '0.6rem 0.8rem', fontSize: '0.78rem', color: '#666', width: 160 }}>DIMENSION</th>
            {TARGET_ROLES.map((r) => (
              <th key={r} style={{ textAlign: 'center', padding: '0.6rem 0.8rem', fontSize: '0.78rem', color: '#666' }}>
                {r.replace('_', ' ').toUpperCase()}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {DIMENSIONS.map((dim) => (
            <tr key={dim} style={{ borderBottom: '1px solid #f0f0f0' }}>
              <td style={{ padding: '0.7rem 0.8rem', fontWeight: 500, fontSize: '0.88rem' }}>{DIM_LABELS[dim]}</td>
              {TARGET_ROLES.map((targetRole) => {
                const rule = ruleForScope(permissions, scope, scopeId, dim, targetRole)
                const isLocked = rule?.locked ?? false
                const hasWindow = !!(rule?.lock_from && rule?.lock_until)
                return (
                  <td key={targetRole} style={{ textAlign: 'center', padding: '0.6rem' }}>
                    <button
                      onClick={() => openCell(dim, targetRole)}
                      disabled={scope === 'network' && !canEditNetwork}
                      style={{
                        background: isLocked ? '#fee2e2' : '#dcfce7',
                        color: isLocked ? '#991b1b' : '#166534',
                        border: 'none', borderRadius: 6,
                        padding: '0.35rem 0.65rem',
                        cursor: scope === 'network' && !canEditNetwork ? 'default' : 'pointer',
                        fontSize: '0.8rem', fontWeight: 600,
                        minWidth: 90,
                      }}
                    >
                      {isLocked
                        ? (hasWindow ? `\u{1F512} ${rule!.lock_from!.slice(0, 5)}–${rule!.lock_until!.slice(0, 5)}` : '\u{1F512} verrouillé')
                        : '\u{1F513} libre'}
                    </button>
                  </td>
                )
              })}
            </tr>
          ))}
        </tbody>
      </table>

      {/* Formulaire d'édition d'une cellule */}
      {editing && (
        <div style={{ background: '#fff', border: '1px solid #e2e8f0', borderRadius: 8, padding: '1.25rem', maxWidth: 440 }}>
          <h4 style={{ marginTop: 0, marginBottom: '0.75rem', fontSize: '0.95rem' }}>
            {DIM_LABELS[editing.dimension]} — {editing.target_role.replace('_', ' ')}
          </h4>

          <label style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: '1rem', cursor: 'pointer' }}>
            <input
              type="checkbox"
              checked={editing.locked}
              onChange={(e) => setEditing((prev) => prev ? { ...prev, locked: e.target.checked } : prev)}
            />
            <span style={{ fontWeight: 600 }}>Verrouillé</span>
          </label>

          {editing.locked && (
            <div style={{ display: 'flex', gap: 8, marginBottom: '1rem', alignItems: 'center' }}>
              <div style={{ flex: 1 }}>
                <label style={{ display: 'block', fontSize: '0.82rem', marginBottom: 3 }}>De</label>
                <input
                  type="time"
                  value={editing.lock_from}
                  onChange={(e) => setEditing((prev) => prev ? { ...prev, lock_from: e.target.value } : prev)}
                  style={{ padding: '0.4rem', border: '1px solid #ddd', borderRadius: 4, width: '100%' }}
                />
              </div>
              <div style={{ flex: 1 }}>
                <label style={{ display: 'block', fontSize: '0.82rem', marginBottom: 3 }}>À</label>
                <input
                  type="time"
                  value={editing.lock_until}
                  onChange={(e) => setEditing((prev) => prev ? { ...prev, lock_until: e.target.value } : prev)}
                  style={{ padding: '0.4rem', border: '1px solid #ddd', borderRadius: 4, width: '100%' }}
                />
              </div>
            </div>
          )}

          <div style={{ marginBottom: '1rem' }}>
            <label style={{ display: 'block', fontSize: '0.82rem', marginBottom: 3 }}>Motif (optionnel)</label>
            <input
              style={{ padding: '0.4rem 0.6rem', border: '1px solid #ddd', borderRadius: 4, width: '100%', boxSizing: 'border-box' as const }}
              value={editing.reason}
              onChange={(e) => setEditing((prev) => prev ? { ...prev, reason: e.target.value } : prev)}
              placeholder="ex: pas de remise pendant le service"
            />
          </div>

          {error && <p style={{ color: '#e53e3e', fontSize: '0.85rem', marginBottom: '0.75rem' }}>{error}</p>}

          <div style={{ display: 'flex', gap: 8 }}>
            <button
              onClick={saveCell}
              disabled={saving}
              style={{ background: '#4f8ef7', color: '#fff', border: 'none', borderRadius: 6, padding: '0.5rem 1rem', cursor: 'pointer', fontWeight: 600 }}
            >
              {saving ? '…' : 'Enregistrer'}
            </button>
            <button
              onClick={() => setEditing(null)}
              style={{ background: '#f5f6fa', border: '1px solid #ddd', borderRadius: 6, padding: '0.5rem 0.8rem', cursor: 'pointer' }}
            >
              Annuler
            </button>
          </div>
        </div>
      )}
    </div>
  )
}
