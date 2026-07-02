import { useEffect, useState } from 'react'
import { getConfig, setProfile } from '../api'

export default function ConfigPage() {
  const [profile, setLocalProfile] = useState<string>('normal')
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState(false)

  useEffect(() => {
    getConfig()
      .then((c) => { setLocalProfile(c.active_profile); setLoading(false) })
      .catch((e: unknown) => {
        setError(e instanceof Error ? e.message : 'Erreur de chargement')
        setLoading(false)
      })
  }, [])

  const handleSwitch = async () => {
    const next = profile === 'normal' ? 'rush' : 'normal'
    setSaving(true); setError(null); setSuccess(false)
    try {
      await setProfile(next)
      setLocalProfile(next)
      setSuccess(true)
      setTimeout(() => setSuccess(false), 2000)
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Erreur de sauvegarde')
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="min-h-screen bg-gray-900 text-white flex items-center justify-center">
      <div className="bg-gray-800 rounded-xl p-8 w-80 shadow-2xl border border-gray-700">
        <h1 className="text-xl font-bold mb-6 text-center tracking-wide">⚙ Configuration KDS</h1>

        {loading && <p className="text-center text-gray-400">Chargement…</p>}

        {!loading && (
          <>
            <div className="mb-6">
              <p className="text-sm text-gray-400 mb-3">Profil actif</p>
              <div className="flex gap-4 justify-center">
                <label className="flex items-center gap-2 cursor-pointer">
                  <input
                    type="radio"
                    name="profile"
                    value="normal"
                    checked={profile === 'normal'}
                    onChange={() => { /* Géré par le bouton basculer */ }}
                    className="accent-blue-500"
                    readOnly
                  />
                  <span className={profile === 'normal' ? 'text-white font-bold' : 'text-gray-400'}>Normal</span>
                </label>
                <label className="flex items-center gap-2 cursor-pointer">
                  <input
                    type="radio"
                    name="profile"
                    value="rush"
                    checked={profile === 'rush'}
                    onChange={() => { /* Géré par le bouton basculer */ }}
                    className="accent-orange-500"
                    readOnly
                  />
                  <span className={profile === 'rush' ? 'text-orange-400 font-bold text-lg' : 'text-gray-400'}>
                    RUSH
                  </span>
                </label>
              </div>
            </div>

            <button
              onClick={handleSwitch}
              disabled={saving}
              className={`w-full py-3 rounded-lg font-bold text-white transition-colors disabled:opacity-50
                ${profile === 'normal'
                  ? 'bg-orange-600 hover:bg-orange-700'
                  : 'bg-blue-600 hover:bg-blue-700'}`}
            >
              {saving ? 'Sauvegarde…' : profile === 'normal' ? 'Basculer en mode RUSH' : 'Basculer en mode Normal'}
            </button>

            {error && <p className="mt-3 text-red-400 text-sm text-center">{error}</p>}
            {success && <p className="mt-3 text-green-400 text-sm text-center">✓ Profil mis à jour</p>}
          </>
        )}
      </div>
    </div>
  )
}
