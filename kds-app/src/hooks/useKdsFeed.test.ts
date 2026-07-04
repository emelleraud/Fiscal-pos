import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { startHeartbeat } from './useKdsFeed'

const mockFetch = vi.fn().mockResolvedValue({})
vi.stubGlobal('fetch', mockFetch)

describe('startHeartbeat', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    mockFetch.mockClear()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('sends heartbeat every 10 seconds', () => {
    const stop = startHeartbeat('grill', 'http://localhost:8080')

    expect(mockFetch).not.toHaveBeenCalled()

    vi.advanceTimersByTime(10_000)
    expect(mockFetch).toHaveBeenCalledTimes(1)
    expect(mockFetch).toHaveBeenCalledWith(
      'http://localhost:8080/api/v1/kds/heartbeat/grill',
      { method: 'POST' }
    )

    vi.advanceTimersByTime(10_000)
    expect(mockFetch).toHaveBeenCalledTimes(2)

    vi.advanceTimersByTime(10_000)
    expect(mockFetch).toHaveBeenCalledTimes(3)

    stop()
  })

  it('stops sending after stop() is called', () => {
    const stop = startHeartbeat('grill', 'http://localhost:8080')

    vi.advanceTimersByTime(10_000)
    expect(mockFetch).toHaveBeenCalledTimes(1)

    stop()
    mockFetch.mockClear()

    vi.advanceTimersByTime(30_000)
    expect(mockFetch).not.toHaveBeenCalled()
  })
})
