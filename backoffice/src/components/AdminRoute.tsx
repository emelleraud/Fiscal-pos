import { Navigate, Outlet } from 'react-router-dom'
import { useAuth } from '../context/AuthContext'

export default function AdminRoute() {
  const { role } = useAuth()
  const allowed = role === 'pos_admin' || role === 'regional_director'
  return allowed ? <Outlet /> : <Navigate to="/dashboard" replace />
}
