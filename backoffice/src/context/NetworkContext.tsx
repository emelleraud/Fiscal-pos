// backoffice/src/context/NetworkContext.tsx
import { createContext, useContext, useEffect, useState, type ReactNode } from 'react'
import { supabase } from '../supabaseClient'
import { useAuth } from './AuthContext'

export interface NetworkSite {
  id: string
  site_code: string
  name: string
  address: string | null
  siret: string | null
}

export interface GroupWithMembers {
  id: string
  name: string
  site_ids: string[]
}

export interface NetworkPermission {
  id: string
  site_id: string | null
  group_id: string | null
  dimension: string
  target_role: string
  locked: boolean
  lock_from: string | null
  lock_until: string | null
  reason: string | null
  updated_by: string | null
  updated_at: string
}

interface NetworkContextValue {
  allSites: NetworkSite[]
  groups: GroupWithMembers[]
  permissions: NetworkPermission[]
  isLocked: (dimension: string, targetRole: string, siteId: string | null) => boolean
  reload: () => void
}

const NetworkContext = createContext<NetworkContextValue>({
  allSites: [],
  groups: [],
  permissions: [],
  isLocked: () => false,
  reload: () => {},
})

function timeToMinutes(t: string): number {
  const [h, m] = t.split(':').map(Number)
  return h * 60 + m
}

function isInWindow(lockFrom: string, lockUntil: string): boolean {
  const now = new Date()
  const cur = now.getHours() * 60 + now.getMinutes()
  return cur >= timeToMinutes(lockFrom) && cur <= timeToMinutes(lockUntil)
}

function checkLocked(rule: NetworkPermission): boolean {
  if (!rule.locked) return false
  if (rule.lock_from && rule.lock_until) return isInWindow(rule.lock_from, rule.lock_until)
  return true
}

export function NetworkProvider({ children }: { children: ReactNode }) {
  const { session, role } = useAuth()
  const [allSites, setAllSites] = useState<NetworkSite[]>([])
  const [groups, setGroups] = useState<GroupWithMembers[]>([])
  const [permissions, setPermissions] = useState<NetworkPermission[]>([])
  const [tick, setTick] = useState(0)

  const isAdmin = role === 'pos_admin' || role === 'regional_director'

  useEffect(() => {
    if (!session || !isAdmin) return

    supabase
      .from('sites')
      .select('id,site_code,name,address,siret')
      .order('site_code')
      .then(({ data }) => setAllSites((data as NetworkSite[]) ?? []))

    supabase
      .from('restaurant_groups')
      .select('id,name,restaurant_group_members(site_id)')
      .order('name')
      .then(({ data }) => {
        setGroups(
          (data ?? []).map((g: { id: string; name: string; restaurant_group_members: Array<{ site_id: string }> }) => ({
            id: g.id,
            name: g.name,
            site_ids: g.restaurant_group_members.map((m) => m.site_id),
          }))
        )
      })

    supabase
      .from('network_permissions')
      .select('*')
      .then(({ data }) => setPermissions((data as NetworkPermission[]) ?? []))
  }, [session, isAdmin, tick])

  function isLocked(dimension: string, targetRole: string, siteId: string | null): boolean {
    if (siteId) {
      const siteRule = permissions.find(
        (p) => p.site_id === siteId && p.group_id === null && p.dimension === dimension && p.target_role === targetRole
      )
      if (siteRule) return checkLocked(siteRule)

      for (const group of groups) {
        if (!group.site_ids.includes(siteId)) continue
        const groupRule = permissions.find(
          (p) => p.group_id === group.id && p.site_id === null && p.dimension === dimension && p.target_role === targetRole
        )
        if (groupRule) return checkLocked(groupRule)
      }
    }

    const networkRule = permissions.find(
      (p) => p.site_id === null && p.group_id === null && p.dimension === dimension && p.target_role === targetRole
    )
    if (networkRule) return checkLocked(networkRule)

    return false
  }

  return (
    <NetworkContext.Provider value={{ allSites, groups, permissions, isLocked, reload: () => setTick((t) => t + 1) }}>
      {children}
    </NetworkContext.Provider>
  )
}

export const useNetwork = () => useContext(NetworkContext)
