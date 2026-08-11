import { useState } from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { CodeInput } from "@/components/CodeInput"
import type { TestStep, Block, Param } from "@/types"
import { DND_REORDER, DND_BLOCK_SLUG, gapForRow } from "@/lib/dnd"
import {
  Trash2,
  Plus,
  FileCode2,
  Play,
  Loader2,
  Bug,
  Wand2,
  Blocks,
  Save,
  GripVertical,
} from "lucide-react"

const PLACEHOLDER_RE = /\{\{([^}]+)\}\}/g

function blockPlaceholders(block: Block): string[] {
  const seen = new Set<string>()
  for (const step of block.steps) {
    for (const text of [step.action, step.assertion]) {
      for (const m of (text ?? "").matchAll(PLACEHOLDER_RE)) seen.add(m[1])
    }
  }
  return Array.from(seen)
}

export function StepsView({
  entries,
  blocks,
  params,
  testName,
  dirty,
  selected,
  running,
  chatInput,
  setChatInput,
  chatBusy,
  updateAt,
  deleteAt,
  addEntry,
  moveStep,
  insertBlockAt,
  toggleSelect,
  saveSelectionAsBlock,
  pauseAt,
  submitChat,
  validate,
  runTest,
  saveTest,
  saveTestAs,
}: {
  entries: TestStep[]
  blocks: [string, Block][]
  params: Param[]
  testName: string
  dirty: boolean
  selected: Set<number>
  running: boolean
  chatInput: string
  setChatInput: (v: string) => void
  chatBusy: boolean
  updateAt: (i: number, next: TestStep) => void
  deleteAt: (i: number) => void
  addEntry: () => void
  moveStep: (from: number, to: number) => void
  insertBlockAt: (gap: number, slug: string) => void
  toggleSelect: (i: number) => void
  saveSelectionAsBlock: () => void
  pauseAt: (i: number) => void
  submitChat: () => void
  validate: () => void
  runTest: () => void
  saveTest: () => void
  saveTestAs: () => void
}) {
  const blocksBySlug = new Map(blocks)
  const verifiedCount = entries.filter((e) => e.kind === "step" && e.assertion).length
  const [dragIndex, setDragIndex] = useState<number | null>(null)
  const [overGap, setOverGap] = useState<number | null>(null)

  function onRowDragOver(e: React.DragEvent, i: number) {
    if (!e.dataTransfer.types.includes(DND_REORDER) && !e.dataTransfer.types.includes(DND_BLOCK_SLUG)) return
    e.preventDefault()
    setOverGap(gapForRow(e, i))
  }

  function onRowDrop(e: React.DragEvent, i: number) {
    e.preventDefault()
    const gap = gapForRow(e, i)
    const slug = e.dataTransfer.getData(DND_BLOCK_SLUG)
    if (slug) {
      insertBlockAt(gap, slug)
    } else if (dragIndex !== null) {
      moveStep(dragIndex, gap)
    }
    setDragIndex(null)
    setOverGap(null)
  }

  function endDrag() {
    setDragIndex(null)
    setOverGap(null)
  }

  return (
    <div className="flex-1 min-w-0 flex flex-col gap-4">
      <div className="flex items-center justify-between sticky top-0 bg-background/95 backdrop-blur-sm py-3 z-10 border-b border-border">
        <div className="flex items-center gap-5 min-w-0">
          <h2 className="font-mono text-[13px] font-semibold truncate">
            {testName}
            {dirty && <span className="text-primary"> •</span>}
          </h2>
          {entries.length > 0 && (
            <div className="flex gap-4 font-mono text-[11px] text-muted-foreground shrink-0">
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
        <div className="flex gap-2 shrink-0">
          {selected.size > 0 && (
            <Button variant="outline" onClick={saveSelectionAsBlock}>
              <Save /> Save selection as block
            </Button>
          )}
          <Button variant="outline" onClick={saveTestAs}>
            <Save /> Save as
          </Button>
          <Button variant="outline" onClick={saveTest} disabled={!dirty}>
            <Save /> Save
          </Button>
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
        <div
          onDragOver={(e) => {
            if (!e.dataTransfer.types.includes(DND_BLOCK_SLUG)) return
            e.preventDefault()
            setOverGap(0)
          }}
          onDrop={(e) => onRowDrop(e, 0)}
          onDragLeave={endDrag}
          className={
            "rounded-md border border-dashed p-6 text-center font-mono text-sm text-muted-foreground italic " +
            (overGap === 0 ? "border-primary bg-primary/5" : "border-border")
          }
        >
          // no actions captured yet — add a step, or drag a block in from the right
        </div>
      )}

      {entries.length > 0 && (
        <div className="overflow-x-auto rounded-md border border-border">
          <table className="w-full border-collapse font-mono text-xs">
            <thead>
              <tr>
                <th className="w-6" />
                <th className="w-6" />
                <th className="w-8" />
                <th className="bg-muted/40 border-b border-border px-3.5 py-2 text-left text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
                  step
                </th>
                <th className="w-[6.5rem]" />
              </tr>
            </thead>
            <tbody>
              {entries.map((entry, i) => {
                const isBlock = entry.kind === "block"
                const block = isBlock ? blocksBySlug.get(entry.slug) : undefined
                return (
                  <tr
                    key={i}
                    onDragOver={(e) => onRowDragOver(e, i)}
                    onDrop={(e) => onRowDrop(e, i)}
                    onDragLeave={() => setOverGap(null)}
                    className={
                      (overGap === i ? "border-t-2 border-t-primary " : "") +
                      (isBlock ? "bg-primary/5" : i % 2 === 1 ? "bg-muted/20" : "")
                    }
                  >
                    <td className="border-b border-border/60 px-1 py-2 align-top">
                      <span
                        draggable
                        onDragStart={(e) => {
                          setDragIndex(i)
                          e.dataTransfer.setData(DND_REORDER, String(i))
                          e.dataTransfer.effectAllowed = "move"
                        }}
                        onDragEnd={endDrag}
                        className="cursor-grab active:cursor-grabbing text-muted-foreground inline-flex"
                        title="Drag to reorder"
                        aria-label="Drag to reorder step"
                      >
                        <GripVertical className="size-3.5" />
                      </span>
                    </td>
                    <td className="border-b border-border/60 px-2 py-2 align-top">
                      {!isBlock && (
                        <input
                          type="checkbox"
                          checked={selected.has(i)}
                          onChange={() => toggleSelect(i)}
                          aria-label="Select step"
                        />
                      )}
                    </td>
                    <td className="border-b border-border/60 px-3 py-2 text-muted-foreground tabular-nums align-top">
                      {String(i + 1).padStart(2, "0")}
                    </td>
                    <td className="border-b border-border/60 px-1.5 py-1 align-top">
                      {isBlock ? (
                        <div className="flex flex-col gap-1.5">
                          <div className="flex items-center gap-1.5">
                            <Blocks className="size-3.5 text-primary" />
                            <span className="font-medium">{block?.name ?? entry.slug}</span>
                            {!block && (
                              <span className="text-muted-foreground italic">(unknown block: {entry.slug})</span>
                            )}
                          </div>
                          {block &&
                            blockPlaceholders(block).map((placeholder) => (
                              <div key={placeholder} className="flex items-center gap-2 pl-5">
                                <span className="text-muted-foreground">{`{{${placeholder}}}`}</span>
                                <select
                                  className="h-6 rounded-md border border-input bg-transparent px-1 text-xs"
                                  value={entry.bindings[placeholder] ?? ""}
                                  onChange={(e) =>
                                    updateAt(i, {
                                      ...entry,
                                      bindings: { ...entry.bindings, [placeholder]: e.target.value },
                                    })
                                  }
                                >
                                  <option value="" disabled>
                                    select param…
                                  </option>
                                  {params.map((p) => (
                                    <option key={p.name} value={p.name}>
                                      {p.name}
                                    </option>
                                  ))}
                                </select>
                              </div>
                            ))}
                        </div>
                      ) : (
                        <div className="flex flex-col gap-0.5">
                          <CodeInput value={entry.action ?? ""} onChange={(v) => updateAt(i, { ...entry, action: v })} />
                          <CodeInput
                            value={entry.assertion ?? ""}
                            placeholder="await expect(...)"
                            onChange={(v) => updateAt(i, { ...entry, assertion: v })}
                            muted
                          />
                        </div>
                      )}
                    </td>
                    <td className="border-b border-border/60 px-2 py-1 align-top">
                      <div className="flex gap-0.5">
                        {!isBlock && (
                          <Button
                            variant="ghost"
                            size="icon"
                            onClick={() => pauseAt(i)}
                            aria-label="Debug: pause here and inspect the live DOM"
                            title="Debug: pause here and open Playwright Inspector"
                          >
                            <Bug className="size-3.5" />
                          </Button>
                        )}
                        <Button variant="ghost" size="icon" onClick={() => deleteAt(i)} aria-label="Delete action">
                          <Trash2 className="size-3.5" />
                        </Button>
                      </div>
                    </td>
                  </tr>
                )
              })}
              <tr
                onDragOver={(e) => {
                  if (!e.dataTransfer.types.includes(DND_REORDER) && !e.dataTransfer.types.includes(DND_BLOCK_SLUG))
                    return
                  e.preventDefault()
                  setOverGap(entries.length)
                }}
                onDrop={(e) => onRowDrop(e, entries.length)}
                onDragLeave={() => setOverGap(null)}
                className={overGap === entries.length ? "border-t-2 border-t-primary" : undefined}
              >
                <td colSpan={5} className="h-3" />
              </tr>
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}
