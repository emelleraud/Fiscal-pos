interface Props {
  connected: boolean
}

export default function ConnectionBanner({ connected }: Props) {
  if (connected) return null
  return (
    <div className="fixed top-0 left-0 right-0 z-50 bg-red-700 text-white text-center py-2 text-sm font-semibold animate-pulse">
      ⚠ Connexion SSE perdue — reconnexion en cours…
    </div>
  )
}
