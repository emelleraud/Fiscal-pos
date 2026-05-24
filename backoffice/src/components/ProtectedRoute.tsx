import { Navigate, Outlet } from 'react-router-dom'
import { useAuth } from '../context/AuthContext'

export default function ProtectedRoute() {
  const { session, loading } = useAuth()
  if (loading) return <p style={{ padding: '2rem', color: '#888' }}>Chargement…</p>
  if (!session) return <Navigate to="/login" replace />
  return <Outlet />
}
