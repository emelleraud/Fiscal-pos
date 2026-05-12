import { NavLink } from 'react-router-dom'
import type { ReactNode } from 'react'

const navItems = [
  { to: '/dashboard',      label: '📊 Dashboard CA' },
  { to: '/fiscal-journal', label: '📋 Journal fiscal' },
  { to: '/z-reports',      label: '🧾 Rapports Z' },
]

export default function Layout({ children }: { children: ReactNode }) {
  return (
    <div style={{ display: 'flex', height: '100vh', fontFamily: 'system-ui', fontSize: '14px' }}>
      <nav style={{
        width: 220, flexShrink: 0,
        background: '#1a1a2e', padding: '1.5rem 1rem',
        color: '#fff', display: 'flex', flexDirection: 'column', gap: '0.25rem'
      }}>
        <div style={{ marginBottom: '1.5rem', opacity: 0.5, fontSize: '0.75rem', textTransform: 'uppercase', letterSpacing: '0.1em' }}>
          POS Back-office
        </div>
        {navItems.map(({ to, label }) => (
          <NavLink
            key={to}
            to={to}
            style={({ isActive }) => ({
              display: 'block',
              padding: '0.6rem 0.75rem',
              borderRadius: 6,
              textDecoration: 'none',
              color: isActive ? '#fff' : '#888',
              background: isActive ? '#16213e' : 'transparent',
              borderLeft: isActive ? '3px solid #4f8ef7' : '3px solid transparent',
              transition: 'all 0.15s',
            })}
          >
            {label}
          </NavLink>
        ))}
      </nav>
      <main style={{
        flex: 1, padding: '2rem', overflowY: 'auto',
        background: '#f8f9fa', minWidth: 0
      }}>
        {children}
      </main>
    </div>
  )
}
