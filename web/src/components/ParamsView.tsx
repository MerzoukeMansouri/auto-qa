import { useRef } from "react"
import { Button } from "@/components/ui/button"
import { CodeInput } from "@/components/CodeInput"
import type { Param } from "@/types"
import { Trash2, Plus } from "lucide-react"

export function ParamsView({ params, setParams }: { params: Param[]; setParams: (p: Param[]) => void }) {
  const saveTimer = useRef<number | undefined>(undefined)

  function scheduleSave(next: Param[]) {
    setParams(next)
    window.clearTimeout(saveTimer.current)
    saveTimer.current = window.setTimeout(() => {
      fetch("/api/params", {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(next),
      })
    }, 400)
  }

  function updateAt(i: number, next: Param) {
    const copy = params.slice()
    copy[i] = next
    scheduleSave(copy)
  }

  function deleteAt(i: number) {
    scheduleSave(params.filter((_, idx) => idx !== i))
  }

  function addParam() {
    scheduleSave([...params, { name: "", value: "" }])
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex justify-end">
        <Button variant="outline" onClick={addParam}>
          <Plus /> Add param
        </Button>
      </div>

      {params.length === 0 && (
        <p className="font-mono text-sm text-muted-foreground italic">// no params yet</p>
      )}

      {params.length > 0 && (
        <div className="overflow-x-auto rounded-md border border-border">
          <table className="w-full border-collapse font-mono text-xs">
            <thead>
              <tr>
                <th className="bg-muted/40 border-b border-border px-3.5 py-2 text-left text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
                  name
                </th>
                <th className="bg-muted/40 border-b border-border px-3.5 py-2 text-left text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
                  value
                </th>
                <th className="w-9" />
              </tr>
            </thead>
            <tbody>
              {params.map((p, i) => (
                <tr key={i} className={i % 2 === 1 ? "bg-muted/20" : undefined}>
                  <td className="border-b border-border/60 px-1.5 py-1 align-top">
                    <CodeInput value={p.name} onChange={(v) => updateAt(i, { ...p, name: v })} />
                  </td>
                  <td className="border-b border-border/60 px-1.5 py-1 align-top">
                    <CodeInput value={p.value} onChange={(v) => updateAt(i, { ...p, value: v })} />
                  </td>
                  <td className="border-b border-border/60 px-2 py-1 align-top">
                    <Button variant="ghost" size="icon" onClick={() => deleteAt(i)} aria-label="Delete param">
                      <Trash2 className="size-3.5" />
                    </Button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}
