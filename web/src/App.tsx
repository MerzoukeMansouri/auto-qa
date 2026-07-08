import { useEffect, useRef, useState } from "react"
import { Button } from "@/components/ui/button"
import { Card, CardHeader, CardContent } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Badge } from "@/components/ui/badge"
import { Dialog, DialogTrigger, DialogContent } from "@/components/ui/dialog"
import type { ActionEntry } from "@/types"
import { emptyAssert } from "@/types"
import { Trash2, Plus, CheckCircle2 } from "lucide-react"

function actionSummary(e: ActionEntry): string {
  if (e.selector) return e.selector
  if (e.url) return e.url
  if (e.combo) return e.combo
  if (typeof e.x === "number") return `(${e.x}, ${e.y})`
  return "—"
}

function ActionCard({
  entry,
  index,
  onChange,
  onDelete,
}: {
  entry: ActionEntry
  index: number
  onChange: (next: ActionEntry) => void
  onDelete: () => void
}) {
  const isAssert = entry.kind === "assert"
  return (
    <Card>
      <CardHeader>
        <div className="flex items-center gap-3">
          <span className="text-muted-foreground text-sm w-6 text-right">{index + 1}</span>
          <Badge variant={isAssert ? "assert" : "secondary"}>{entry.kind}</Badge>
          <span className="text-sm text-muted-foreground truncate max-w-[24rem]">
            {actionSummary(entry)}
          </span>
        </div>
        <Button variant="ghost" size="icon" onClick={onDelete} aria-label="Delete action">
          <Trash2 />
        </Button>
      </CardHeader>
      <CardContent className="flex gap-4 items-start">
        {entry.screenshot && (
          <Dialog>
            <DialogTrigger asChild>
              <button className="shrink-0 rounded-md overflow-hidden border border-border">
                <img
                  src={`/screenshots/${entry.screenshot.replace(/^screenshots\//, "")}`}
                  alt=""
                  className="h-20 w-32 object-cover"
                />
              </button>
            </DialogTrigger>
            <DialogContent>
              <img
                src={`/screenshots/${entry.screenshot.replace(/^screenshots\//, "")}`}
                alt=""
                className="max-w-full max-h-[80vh]"
              />
            </DialogContent>
          </Dialog>
        )}
        <div className="flex-1 grid grid-cols-2 gap-3">
          {isAssert ? (
            <>
              <label className="flex flex-col gap-1 text-xs text-muted-foreground">
                Selector
                <Input
                  value={entry.selector ?? ""}
                  onChange={(e) => onChange({ ...entry, selector: e.target.value })}
                />
              </label>
              <label className="flex flex-col gap-1 text-xs text-muted-foreground">
                Assert kind
                <select
                  className="h-8 rounded-md border border-input bg-transparent px-2 text-sm"
                  value={entry.assert_kind ?? "visible"}
                  onChange={(e) => onChange({ ...entry, assert_kind: e.target.value })}
                >
                  <option value="visible">visible</option>
                  <option value="text">text</option>
                  <option value="value">value</option>
                </select>
              </label>
              {entry.assert_kind !== "visible" && (
                <label className="flex flex-col gap-1 text-xs text-muted-foreground col-span-2">
                  Expected value
                  <Input
                    value={entry.value ?? ""}
                    onChange={(e) => onChange({ ...entry, value: e.target.value })}
                  />
                </label>
              )}
            </>
          ) : (
            <>
              {["click", "double_click", "triple_click", "right_click", "middle_click", "hover", "drag", "type"].includes(
                entry.kind
              ) && (
                <label className="flex flex-col gap-1 text-xs text-muted-foreground col-span-2">
                  Selector
                  <Input
                    value={entry.selector ?? ""}
                    placeholder="(no selector captured)"
                    onChange={(e) => onChange({ ...entry, selector: e.target.value })}
                  />
                </label>
              )}
              {entry.kind === "type" && (
                <label className="flex flex-col gap-1 text-xs text-muted-foreground col-span-2">
                  Value
                  <Input
                    value={entry.value ?? ""}
                    onChange={(e) => onChange({ ...entry, value: e.target.value })}
                  />
                </label>
              )}
              {entry.kind === "navigate" && (
                <label className="flex flex-col gap-1 text-xs text-muted-foreground col-span-2">
                  URL
                  <Input
                    value={entry.url ?? ""}
                    onChange={(e) => onChange({ ...entry, url: e.target.value })}
                  />
                </label>
              )}
            </>
          )}
        </div>
      </CardContent>
    </Card>
  )
}

export default function App() {
  const [entries, setEntries] = useState<ActionEntry[]>([])
  const [toast, setToast] = useState<string | null>(null)
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

  function addAssertion() {
    scheduleSave([...entries, emptyAssert()])
  }

  async function validate() {
    window.clearTimeout(saveTimer.current)
    await fetch("/api/actions", {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(entries),
    })
    const res = await fetch("/api/validate", { method: "POST" })
    const data = await res.json()
    setToast(`Wrote ${data.path}`)
    setTimeout(() => setToast(null), 4000)
  }

  return (
    <div className="min-h-screen max-w-3xl mx-auto p-6 flex flex-col gap-4">
      <div className="flex items-center justify-between sticky top-0 bg-background py-2 z-10">
        <h1 className="text-xl font-medium">cua review</h1>
        <div className="flex gap-2">
          <Button variant="outline" onClick={addAssertion}>
            <Plus /> Add assertion
          </Button>
          <Button onClick={validate}>
            <CheckCircle2 /> Validate
          </Button>
        </div>
      </div>

      {entries.length === 0 && (
        <p className="text-muted-foreground text-sm">No actions captured yet.</p>
      )}

      <div className="flex flex-col gap-3">
        {entries.map((entry, i) => (
          <ActionCard
            key={i}
            entry={entry}
            index={i}
            onChange={(next) => updateAt(i, next)}
            onDelete={() => deleteAt(i)}
          />
        ))}
      </div>

      {toast && (
        <div className="fixed bottom-4 right-4 bg-card border border-border rounded-md px-4 py-2 shadow-lg text-sm">
          {toast}
        </div>
      )}
    </div>
  )
}
