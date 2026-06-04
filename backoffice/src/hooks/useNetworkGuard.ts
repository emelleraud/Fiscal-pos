import { useNetwork } from '../context/NetworkContext'
import { useAuth } from '../context/AuthContext'
import { useSite } from '../context/SiteContext'

export function useNetworkGuard(dimension: string): { locked: boolean } {
  const { isLocked } = useNetwork()
  const { role } = useAuth()
  const { activeSiteId } = useSite()
  return { locked: isLocked(dimension, role ?? '', activeSiteId) }
}
