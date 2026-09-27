import { useCallback, useRef } from "react"

/**
 * Debounced PUT: `save(key, url, payload)` fires `fetch(url, {PUT, json})`
 * `delay` ms after the last call for that `key` (default 400ms), cancelling
 * any pending call for the same key. `key` lets one hook instance debounce
 * several independent targets at once (e.g. one block per slug); callers
 * with a single target can just pass a fixed key. `cancel(key)` clears a
 * pending save without scheduling a new one — for callers that need to save
 * immediately instead (bypassing the debounce).
 */
export function useDebouncedSave(delay = 400) {
  const timers = useRef<Record<string, number>>({})

  const cancel = useCallback((key: string) => {
    window.clearTimeout(timers.current[key])
  }, [])

  const save = useCallback(
    (key: string, url: string, payload: unknown, onSaved?: () => void) => {
      window.clearTimeout(timers.current[key])
      timers.current[key] = window.setTimeout(() => {
        fetch(url, {
          method: "PUT",
          headers: { "content-type": "application/json" },
          body: JSON.stringify(payload),
        }).then(() => onSaved?.())
      }, delay)
    },
    [delay],
  )

  return { save, cancel }
}
