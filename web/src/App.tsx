import { useEffect, useRef, useState } from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import type { ActionEntry } from "@/types"
import { emptyEntry } from "@/types"
import { Trash2, Plus, FileCode2, Play, Loader2, Pause, Wand2 } from "lucide-react"

function CodeInput({
  value,
  placeholder,
  onChange,
  muted,
}: {
  value: string
  placeholder?: string
  onChange: (v: string) => void
  muted?: boolean
}) {
  return (
    <Input
      className={
        "h-auto border-transparent bg-transparent px-1 py-0.5 font-mono text-xs shadow-none focus-visible:border-input focus-visible:bg-input/20" +
        (muted ? " text-muted-foreground" : "")
      }
      value={value}
      placeholder={placeholder}
      onChange={(e) => onChange(e.target.value)}
    />
  )
}

export default function App() {
  const [entries, setEntries] = useState<ActionEntry[]>([])
  const [toast, setToast] = useState<{ text: string; variant: "info" | "pass" | "fail" } | null>(null)
  const [running, setRunning] = useState(false)
  const [chatInput, setChatInput] = useState("")
  const [chatBusy, setChatBusy] = useState(false)
  const saveTimer = useRef<number | undefined>(undefined)

  useEffect(() => {
    fetch("/api/actions")
      .then((r) => r.json())
      .then(setEntries)
  }, [])

  function scheduleSave(next: ActionEntry[]) {
    setEntries(next)
    window.clearTimeout(saveTimer.current)
    saveTimer.current = window.setTimeout(() => {
      fetch("/api/actions", {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(next),
      })
    }, 400)
  }

  function updateAt(i: number, next: ActionEntry) {
    const copy = entries.slice()
    copy[i] = next
    scheduleSave(copy)
  }

  function deleteAt(i: number) {
    scheduleSave(entries.filter((_, idx) => idx !== i))
  }

  function addEntry() {
    scheduleSave([...entries, emptyEntry()])
  }

  function insertAfter(i: number) {
    const copy = entries.slice()
    copy.splice(i + 1, 0, emptyEntry())
    scheduleSave(copy)
  }

  async function saveNow() {
    window.clearTimeout(saveTimer.current)
    await fetch("/api/actions", {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(entries),
    })
  }

  function showToast(text: string, variant: "info" | "pass" | "fail") {
    setToast({ text, variant })
    setTimeout(() => setToast(null), 5000)
  }

  async function validate() {
    await saveNow()
    const res = await fetch("/api/validate", { method: "POST" })
    const data = await res.json()
    showToast(`wrote ${data.path}`, "info")
  }

  async function pauseAt(i: number) {
    await saveNow()
    const res = await fetch(`/api/pause/${i}`, { method: "POST" })
    if (res.ok) {
      showToast(`opening inspector at step ${i + 1}…`, "info")
    } else {
      showToast(`could not open inspector at step ${i + 1}`, "fail")
    }
  }

  async function submitChat() {
    if (!chatInput.trim() || chatBusy) return
    await saveNow()
    setChatBusy(true)
    try {
      const res = await fetch("/api/chat", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ instruction: chatInput }),
      })
      if (res.ok) {
        setEntries(await res.json())
        setChatInput("")
        showToast("steps updated", "pass")
      } else {
        showToast(await res.text(), "fail")
      }
    } finally {
      setChatBusy(false)
    }
  }

  async function runTest() {
    await saveNow()
    setRunning(true)
    try {
      const res = await fetch("/api/run", { method: "POST" })
      const data = await res.json()
      if (data.passed) {
        showToast("test passed", "pass")
      } else {
        console.error(data.output)
        showToast("test failed — see browser console for output", "fail")
      }
    } finally {
      setRunning(false)
    }
  }

  const verifiedCount = entries.filter((e) => e.assertion).length

  return (
    <div className="min-h-screen max-w-4xl mx-auto p-6 flex flex-col gap-4">
      <div className="flex items-center justify-between sticky top-0 bg-background/95 backdrop-blur-sm py-3 z-10 border-b border-border">
        <div className="flex items-center gap-5">
          <h1 className="font-mono text-[15px] font-semibold">cua_review</h1>
          {entries.length > 0 && (
            <div className="flex gap-4 font-mono text-[11px] text-muted-foreground">
              <span>
                steps <b className="text-foreground tabular-nums">{entries.length}</b>
              </span>
              <span>
                verified <b className="text-foreground tabular-nums">{verifiedCount}</b>
              </span>
              <span>
                unverified <b className="text-foreground tabular-nums">{entries.length - verifiedCount}</b>
              </span>
            </div>
          )}
        </div>
        <div className="flex gap-2">
          <Button variant="outline" onClick={addEntry}>
            <Plus /> Add step
          </Button>
          <Button variant="outline" onClick={validate}>
            <FileCode2 /> Generate
          </Button>
          <Button onClick={runTest} disabled={running}>
            {running ? <Loader2 className="animate-spin" /> : <Play />}
            {running ? "Running…" : "Run"}
          </Button>
        </div>
      </div>

      <div className="flex gap-2">
        <Input
          className="font-mono text-xs"
          placeholder="e.g. add an assertion that the price is visible"
          value={chatInput}
          onChange={(e) => setChatInput(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && submitChat()}
          disabled={chatBusy}
        />
        <Button variant="outline" onClick={submitChat} disabled={chatBusy || !chatInput.trim()}>
          {chatBusy ? <Loader2 className="animate-spin" /> : <Wand2 />}
          {chatBusy ? "Editing…" : "Edit"}
        </Button>
      </div>

      {entries.length === 0 && (
        <p className="font-mono text-sm text-muted-foreground italic">
          // no actions captured yet
        </p>
      )}

      {entries.length > 0 && (
        <div className="overflow-x-auto rounded-md border border-border">
          <table className="w-full border-collapse font-mono text-xs">
            <thead>
              <tr>
                <th className="w-8" />
                <th className="bg-muted/40 border-b border-border px-3.5 py-2 text-left text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
                  step
                </th>
                <th className="w-9" />
              </tr>
            </thead>
            <tbody>
              {entries.map((entry, i) => (
                <tr key={i} className={i % 2 === 1 ? "bg-muted/20" : undefined}>
                  <td className="border-b border-border/60 px-3 py-2 text-muted-foreground tabular-nums align-top">
                    {String(i + 1).padStart(2, "0")}
                  </td>
                  <td className="border-b border-border/60 px-1.5 py-1 align-top">
                    <div className="flex flex-col gap-0.5">
                      <CodeInput value={entry.action} onChange={(v) => updateAt(i, { ...entry, action: v })} />
                      <CodeInput
                        value={entry.assertion}
                        placeholder="await expect(...)"
                        onChange={(v) => updateAt(i, { ...entry, assertion: v })}
                        muted
                      />
                    </div>
                  </td>
                  <td className="border-b border-border/60 px-2 py-1 align-top">
                    <div className="flex gap-0.5">
                      <Button
                        variant="ghost"
                        size="icon"
                        onClick={() => insertAfter(i)}
                        aria-label="Insert step below"
                        title="Insert step below"
                      >
                        <Plus className="size-3.5" />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        onClick={() => pauseAt(i)}
                        aria-label="Pause here and inspect the live DOM"
                        title="Pause here and inspect the live DOM"
                      >
                        <Pause className="size-3.5" />
                      </Button>
                      <Button variant="ghost" size="icon" onClick={() => deleteAt(i)} aria-label="Delete action">
                        <Trash2 className="size-3.5" />
                      </Button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {toast && (
        <div
          className={
            "fixed bottom-4 right-4 bg-card border-y border-r border-border rounded-md px-4 py-2 shadow-lg text-sm font-mono border-l-2 " +
            (toast.variant === "pass"
              ? "border-l-success"
              : toast.variant === "fail"
                ? "border-l-destructive"
                : "border-l-primary")
          }
        >
          {toast.text}
        </div>
      )}
    </div>
  )
}
