import { NavLink, Outlet } from 'react-router-dom'
import { useAuth } from '../context/AuthContext'
import { useSite } from '../context/SiteContext'

type NavItem = { to: string; label: string } | { divider: true; label: string }

const navItems: NavItem[] = [
  { to: '/dashboard',      label: '📊 Dashboard CA' },
  { to: '/fiscal-journal', label: '📋 Journal fiscal' },
  { to: '/z-reports',      label: '🧾 Rapports Z' },
  { divider: true, label: 'Catalogue' },
  { to: '/categories',     label: '🏷️ Catégories' },
  { to: '/products',       label: '🍔 Produits' },
  { to: '/combos',         label: '🎁 Combos' },
  { to: '/modifiers',      label: '🔧 Modificateurs' },
  { divider: true, label: 'Config site' },
  { to: '/menu',           label: '📤 Config publiée' },
]

export default function Layout() {
  const { signOut } = useAuth()
  const { sites, activeSiteId, setActiveSiteId } = useSite()

  return (
    <div style={{ display: 'flex', height: '100vh', fontFamily: 'system-ui', fontSize: '14px' }}>
      <nav style={{
        width: 220, flexShrink: 0,
        background: '#1a1a2e', padding: '1.5rem 1rem',
        color: '#fff', display: 'flex', flexDirection: 'column', gap: '0.15rem',
        overflowY: 'auto',
      }}>
        <div style={{ marginBottom: '1rem', opacity: 0.5, fontSize: '0.75rem', textTransform: 'uppercase', letterSpacing: '0.1em' }}>
          POS Back-office
        </div>

        {sites.length > 1 && (
          <select
            value={activeSiteId ?? ''}
            onChange={e => setActiveSiteId(e.target.value)}
            style={{
              width: '100%', marginBottom: '0.75rem',
              padding: '0.45rem 0.6rem', borderRadius: 6,
              border: '1px solid #333', background: '#16213e',
              color: '#fff', fontSize: '0.8rem', cursor: 'pointer',
            }}
          >
            {sites.map(s => (
              <option key={s.id} value={s.id}>{s.name} ({s.site_code})</option>
            ))}
          </select>
        )}

        {navItems.map((item, i) =>
          'divider' in item ? (
            <div key={item.label} style={{ marginTop: '0.75rem', marginBottom: '0.25rem' }}>
              <div style={{ borderTop: '1px solid #2a2a4a', marginBottom: '0.5rem' }} />
              <span style={{ fontSize: '0.65rem', textTransform: 'uppercase', letterSpacing: '0.12em', color: '#444', paddingLeft: '0.75rem' }}>
                {item.label}
              </span>
            </div>
          ) : (
            <NavLink
              key={item.to}
              to={item.to}
              style={({ isActive }) => ({
                display: 'block',
                padding: '0.55rem 0.75rem',
                borderRadius: 6,
                textDecoration: 'none',
                color: isActive ? '#fff' : '#888',
                background: isActive ? '#16213e' : 'transparent',
                borderLeft: isActive ? '3px solid #4f8ef7' : '3px solid transparent',
                transition: 'all 0.15s',
                fontSize: '0.85rem',
              })}
            >
              {item.label}
            </NavLink>
          )
        )}

        <button
          onClick={signOut}
          style={{
            marginTop: 'auto', padding: '0.5rem 0.75rem',
            borderRadius: 6, border: '1px solid #333',
            background: 'transparent', color: '#666',
            fontSize: '0.8rem', cursor: 'pointer', textAlign: 'left',
          }}
        >
          Déconnexion
        </button>
      </nav>

      <main style={{
        flex: 1, padding: '2rem', overflowY: 'auto',
        background: '#f8f9fa', minWidth: 0,
      }}>
        <Outlet />
      </main>
    </div>
  )
}
