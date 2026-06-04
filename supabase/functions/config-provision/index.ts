import { createClient } from 'https://esm.sh/@supabase/supabase-js@2'

const corsHeaders = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Headers': 'authorization, x-client-info, apikey, content-type',
}

Deno.serve(async (req) => {
  if (req.method === 'OPTIONS') return new Response('ok', { headers: corsHeaders })

  const authHeader = req.headers.get('Authorization')
  if (!authHeader) {
    return new Response(JSON.stringify({ error: 'missing_auth' }), { status: 401, headers: corsHeaders })
  }

  try {
    const supabaseUser = createClient(
      Deno.env.get('SUPABASE_URL')!,
      Deno.env.get('SUPABASE_ANON_KEY')!,
      { global: { headers: { Authorization: authHeader } } }
    )
    const { data: { user }, error: authErr } = await supabaseUser.auth.getUser()
    if (authErr || !user) {
      return new Response(JSON.stringify({ error: 'unauthorized' }), { status: 401, headers: corsHeaders })
    }
    if (user.app_metadata?.role !== 'pos_admin') {
      return new Response(JSON.stringify({ error: 'forbidden' }), { status: 403, headers: corsHeaders })
    }

    const { site_id, key_hex } = await req.json() as { site_id: string; key_hex: string }

    const supabaseAdmin = createClient(
      Deno.env.get('SUPABASE_URL')!,
      Deno.env.get('SUPABASE_SERVICE_ROLE_KEY')!
    )

    const { data: configured_at, error } = await supabaseAdmin.rpc('provision_fiscal_key', {
      p_site_id: site_id,
      p_key_hex: key_hex,
    })

    if (error) {
      const status = error.message.includes('invalid_key_format') ? 400 : 500
      return new Response(JSON.stringify({ error: error.message }), { status, headers: corsHeaders })
    }

    return new Response(JSON.stringify({ configured_at }), {
      status: 200, headers: { ...corsHeaders, 'Content-Type': 'application/json' },
    })
  } catch (e: unknown) {
    console.error('config-provision error:', e)
    return new Response(JSON.stringify({ error: 'internal_error' }), { status: 500, headers: corsHeaders })
  }
})
