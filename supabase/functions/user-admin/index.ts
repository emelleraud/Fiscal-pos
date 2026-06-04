import { createClient } from 'https://esm.sh/@supabase/supabase-js@2'

const corsHeaders = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Headers': 'authorization, x-client-info, apikey, content-type',
}

function generatePassword(length = 16): string {
  const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789'
  const bytes = crypto.getRandomValues(new Uint8Array(length))
  return Array.from(bytes, (b: number) => chars[b % chars.length]).join('')
}

function toSlug(employeeId: string): string {
  return employeeId.toLowerCase().replace(/[^a-z0-9]/g, '')
}

Deno.serve(async (req) => {
  if (req.method === 'OPTIONS') return new Response('ok', { headers: corsHeaders })

  const authHeader = req.headers.get('Authorization')
  if (!authHeader) {
    return new Response(JSON.stringify({ error: 'missing_auth' }), { status: 401, headers: corsHeaders })
  }

  const supabaseUser = createClient(
    Deno.env.get('SUPABASE_URL')!,
    Deno.env.get('SUPABASE_ANON_KEY')!,
    { global: { headers: { Authorization: authHeader } } }
  )
  const { data: { user }, error: authErr } = await supabaseUser.auth.getUser()
  if (authErr || !user) {
    return new Response(JSON.stringify({ error: 'unauthorized' }), { status: 401, headers: corsHeaders })
  }

  const callerRole = user.app_metadata?.role as string | undefined
  if (callerRole !== 'pos_admin' && callerRole !== 'regional_director') {
    return new Response(JSON.stringify({ error: 'forbidden' }), { status: 403, headers: corsHeaders })
  }

  const supabaseAdmin = createClient(
    Deno.env.get('SUPABASE_URL')!,
    Deno.env.get('SUPABASE_SERVICE_ROLE_KEY')!
  )

  // Pour regional_director : vérifie que le site cible est dans son périmètre
  async function assertScope(targetSiteId: string | undefined): Promise<Response | null> {
    if (callerRole === 'pos_admin') return null
    if (!targetSiteId) {
      return new Response(JSON.stringify({ error: 'scope_forbidden' }), { status: 403, headers: corsHeaders })
    }
    const { data: ok } = await supabaseUser.rpc('can_access_site', { p_site_id: targetSiteId })
    if (!ok) {
      return new Response(JSON.stringify({ error: 'scope_forbidden' }), { status: 403, headers: corsHeaders })
    }
    return null
  }

  try {
    const body = await req.json() as { action: string; payload: Record<string, string> }
    const { action, payload } = body

    switch (action) {
      case 'list': {
        const { data, error } = await supabaseUser.rpc('list_admin_users')
        if (error) throw error
        return new Response(JSON.stringify({ users: data }), {
          status: 200, headers: { ...corsHeaders, 'Content-Type': 'application/json' },
        })
      }

      case 'invite': {
        const scopeErr = await assertScope(payload.site_id)
        if (scopeErr) return scopeErr
        const appMeta: Record<string, string> = { role: payload.role, display_name: payload.display_name ?? '' }
        if (payload.site_id) appMeta.site_id = payload.site_id
        const { data, error } = await supabaseAdmin.auth.admin.inviteUserByEmail(payload.email, { data: appMeta })
        if (error) throw error
        return new Response(JSON.stringify({ user_id: data.user.id }), {
          status: 200, headers: { ...corsHeaders, 'Content-Type': 'application/json' },
        })
      }

      case 'create_direct': {
        const scopeErr = await assertScope(payload.site_id)
        if (scopeErr) return scopeErr
        const slug = toSlug(payload.employee_id)
        const email = `emp_${slug}@internal.pos-fiscal.local`
        const tempPassword = generatePassword()
        const appMeta: Record<string, string> = { role: payload.role, display_name: payload.display_name ?? '' }
        if (payload.site_id) appMeta.site_id = payload.site_id
        const { data, error } = await supabaseAdmin.auth.admin.createUser({
          email, password: tempPassword, app_metadata: appMeta, email_confirm: true,
        })
        if (error) throw error
        return new Response(JSON.stringify({ user_id: data.user.id, email, temp_password: tempPassword }), {
          status: 200, headers: { ...corsHeaders, 'Content-Type': 'application/json' },
        })
      }

      case 'update_role': {
        const { data: { user: targetUser }, error: getErr } = await supabaseAdmin.auth.admin.getUserById(payload.user_id)
        if (getErr) throw getErr
        const currentSiteId = targetUser.app_metadata?.site_id as string | undefined
        // Vérifier scope sur le site actuel ET le nouveau site si fourni
        const scopeErr = await assertScope(currentSiteId)
        if (scopeErr) return scopeErr
        if (payload.site_id && payload.site_id !== currentSiteId) {
          const newScopeErr = await assertScope(payload.site_id)
          if (newScopeErr) return newScopeErr
        }
        const appMeta: Record<string, string> = { role: payload.role }
        if (payload.site_id) appMeta.site_id = payload.site_id
        const { error } = await supabaseAdmin.auth.admin.updateUserById(payload.user_id, { app_metadata: appMeta })
        if (error) throw error
        return new Response(JSON.stringify({ ok: true }), {
          status: 200, headers: { ...corsHeaders, 'Content-Type': 'application/json' },
        })
      }

      case 'revoke': {
        const { data: { user: targetUser }, error: getErr } = await supabaseAdmin.auth.admin.getUserById(payload.user_id)
        if (getErr) throw getErr
        const scopeErr = await assertScope(targetUser.app_metadata?.site_id as string | undefined)
        if (scopeErr) return scopeErr
        const { error } = await supabaseAdmin.auth.admin.updateUserById(payload.user_id, { ban_duration: '876600h' })
        if (error) throw error
        return new Response(JSON.stringify({ ok: true }), {
          status: 200, headers: { ...corsHeaders, 'Content-Type': 'application/json' },
        })
      }

      default:
        return new Response(JSON.stringify({ error: 'unknown_action' }), { status: 400, headers: corsHeaders })
    }
  } catch (e: unknown) {
    console.error('user-admin error:', e)
    const msg = e instanceof Error && (e.message === 'scope_forbidden' || e.message === 'unknown_action')
      ? e.message
      : 'internal_error'
    return new Response(JSON.stringify({ error: msg }), { status: 400, headers: corsHeaders })
  }
})
