import { useRef, useState } from "react"
import { Button } from "@/components/ui/button"
import { CodeInput } from "@/components/CodeInput"
import type { Block } from "@/types"
import { Trash2, Plus, GripVertical } from "lucide-react"
import { DND_REORDER, gapForRow, reorder } from "@/lib/dnd"

function slugify(name: string): string {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
}

export function BlocksView({
  blocks,
  refreshBlocks,
  showToast,
}: {
  blocks: [string, Block][]
  refreshBlocks: () => Promise<void>
  showToast: (text: string, variant: "info" | "pass" | "fail") => void
}) {
  const saveTimers = useRef<Record<string, number>>({})
  const [drag, setDrag] = useState<{ slug: string; index: number } | null>(null)
  const [overGap, setOverGap] = useState<{ slug: string; gap: number } | null>(null)

  function scheduleSave(slug: string, block: Block) {
    window.clearTimeout(saveTimers.current[slug])
    saveTimers.current[slug] = window.setTimeout(() => {
      fetch(`/api/blocks/${slug}`, {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(block),
      }).then(refreshBlocks)
    }, 400)
  }

  async function deleteBlock(slug: string) {
    await fetch(`/api/blocks/${slug}`, { method: "DELETE" })
    await refreshBlocks()
    showToast(`deleted block ${slug}`, "info")
  }

  async function newBlock() {
    const name = window.prompt("Block name?")
    if (!name || !name.trim()) return
    const slug = slugify(name)
    const block: Block = { name: name.trim(), steps: [] }
    await fetch(`/api/blocks/${slug}`, {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(block),
    })
    await refreshBlocks()
  }

  function updateStep(slug: string, block: Block, i: number, next: { action: string; assertion: string }) {
    const steps = block.steps.slice()
    steps[i] = next
    scheduleSave(slug, { ...block, steps })
  }

  function addStep(slug: string, block: Block) {
    scheduleSave(slug, { ...block, steps: [...block.steps, { action: "", assertion: "" }] })
  }

  function deleteStep(slug: string, block: Block, i: number) {
    scheduleSave(slug, { ...block, steps: block.steps.filter((_, idx) => idx !== i) })
  }

  function moveStep(slug: string, block: Block, from: number, to: number) {
    scheduleSave(slug, { ...block, steps: reorder(block.steps, from, to) })
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex justify-end">
        <Button variant="outline" onClick={newBlock}>
          <Plus /> New block
        </Button>
      </div>

      {blocks.length === 0 && (
        <p className="font-mono text-sm text-muted-foreground italic">// no blocks yet</p>
      )}

      {blocks.map(([slug, block]) => (
        <div key={slug} className="rounded-md border border-border overflow-hidden">
          <div className="flex items-center justify-between bg-muted/40 border-b border-border px-3.5 py-2">
            <div className="font-mono text-xs">
              <span className="font-medium">{block.name}</span>{" "}
              <span className="text-muted-foreground">
                ({slug}) · {block.steps.length} step{block.steps.length === 1 ? "" : "s"}
              </span>
            </div>
            <Button variant="ghost" size="icon" onClick={() => deleteBlock(slug)} aria-label="Delete block">
              <Trash2 className="size-3.5" />
            </Button>
          </div>
          <table className="w-full border-collapse font-mono text-xs">
            <tbody>
              {block.steps.map((step, i) => (
                <tr
                  key={i}
                  onDragOver={(e) => {
                    if (!e.dataTransfer.types.includes(DND_REORDER)) return
                    e.preventDefault()
                    setOverGap({ slug, gap: gapForRow(e, i) })
                  }}
                  onDrop={(e) => {
                    e.preventDefault()
                    const gap = gapForRow(e, i)
                    if (drag && drag.slug === slug) moveStep(slug, block, drag.index, gap)
                    setDrag(null)
                    setOverGap(null)
                  }}
                  onDragLeave={() => setOverGap(null)}
                  className={
                    (overGap?.slug === slug && overGap.gap === i ? "border-t-2 border-t-primary " : "") +
                    (i % 2 === 1 ? "bg-muted/20" : "")
                  }
                >
                  <td className="border-b border-border/60 px-1 py-2 align-top">
                    <span
                      draggable
                      onDragStart={(e) => {
                        setDrag({ slug, index: i })
                        e.dataTransfer.setData(DND_REORDER, String(i))
                        e.dataTransfer.effectAllowed = "move"
                      }}
                      onDragEnd={() => {
                        setDrag(null)
                        setOverGap(null)
                      }}
                      className="cursor-grab active:cursor-grabbing text-muted-foreground inline-flex"
                      title="Drag to reorder"
                      aria-label="Drag to reorder step"
                    >
                      <GripVertical className="size-3.5" />
                    </span>
                  </td>
                  <td className="border-b border-border/60 px-3 py-2 text-muted-foreground tabular-nums align-top">
                    {String(i + 1).padStart(2, "0")}
                  </td>
                  <td className="border-b border-border/60 px-1.5 py-1 align-top">
                    <div className="flex flex-col gap-0.5">
                      <CodeInput
                        value={step.action ?? ""}
                        onChange={(v) => updateStep(slug, block, i, { ...step, action: v })}
                      />
                      <CodeInput
                        value={step.assertion ?? ""}
                        placeholder="await expect(...)"
                        onChange={(v) => updateStep(slug, block, i, { ...step, assertion: v })}
                        muted
                      />
                    </div>
                  </td>
                  <td className="border-b border-border/60 px-2 py-1 align-top">
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={() => deleteStep(slug, block, i)}
                      aria-label="Delete step"
                    >
                      <Trash2 className="size-3.5" />
                    </Button>
                  </td>
                </tr>
              ))}
              {block.steps.length > 0 && (
                <tr
                  onDragOver={(e) => {
                    if (!e.dataTransfer.types.includes(DND_REORDER)) return
                    e.preventDefault()
                    setOverGap({ slug, gap: block.steps.length })
                  }}
                  onDrop={(e) => {
                    e.preventDefault()
                    if (drag && drag.slug === slug) moveStep(slug, block, drag.index, block.steps.length)
                    setDrag(null)
                    setOverGap(null)
                  }}
                  onDragLeave={() => setOverGap(null)}
                  className={overGap?.slug === slug && overGap.gap === block.steps.length ? "border-t-2 border-t-primary" : undefined}
                >
                  <td colSpan={4} className="h-2" />
                </tr>
              )}
            </tbody>
          </table>
          <div className="px-3.5 py-2 border-t border-border">
            <Button variant="ghost" size="sm" onClick={() => addStep(slug, block)}>
              <Plus className="size-3.5" /> Add step
            </Button>
          </div>
        </div>
      ))}
    </div>
  )
}

export { slugify }
