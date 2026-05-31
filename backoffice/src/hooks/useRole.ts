import { useAuth } from '../context/AuthContext'

export type Role = 'manager' | 'director' | 'regional_director'

const RANK: Record<Role, number> = {
  manager: 1,
  director: 2,
  regional_director: 3,
}

export function useRole() {
  const { role } = useAuth()
  const current = role as Role | null

  const hasRole = (required: Role): boolean => {
    if (!current) return false
    return (RANK[current] ?? 0) >= RANK[required]
  }

  return { role: current, hasRole }
}
