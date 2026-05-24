import { createContext, useContext, useEffect, useState, type ReactNode } from 'react'
import { supabase } from '../supabaseClient'
import { useAuth } from './AuthContext'

interface Site {
  id: string
  site_code: string
  name: string
}

interface SiteContextValue {
  sites: Site[]
  activeSiteId: string | null
  setActiveSiteId: (id: string) => void
}

const SiteContext = createContext<SiteContextValue>({
  sites: [],
  activeSiteId: null,
  setActiveSiteId: () => {},
})

export function SiteProvider({ children }: { children: ReactNode }) {
  const { session } = useAuth()
  const [sites, setSites] = useState<Site[]>([])
  const [activeSiteId, setActiveSiteId] = useState<string | null>(null)

  useEffect(() => {
    if (!session) {
      setSites([])
      setActiveSiteId(null)
      return
    }
    supabase
      .from('sites')
      .select('id,site_code,name')
      .order('site_code')
      .then(({ data }) => {
        if (data && data.length > 0) {
          setSites(data)
          setActiveSiteId(prev => prev ?? data[0].id)
        }
      })
  }, [session])

  return (
    <SiteContext.Provider value={{ sites, activeSiteId, setActiveSiteId }}>
      {children}
    </SiteContext.Provider>
  )
}

export const useSite = () => useContext(SiteContext)
