import type { Block } from "@/types"
import { DND_BLOCK_SLUG } from "@/lib/dnd"
import { Blocks, GripVertical } from "lucide-react"

/** Drag source panel next to the step list: drag a block onto a step row to insert it there. */
export function BlocksPalette({ blocks }: { blocks: [string, Block][] }) {
  return (
    <aside className="w-56 shrink-0 hidden lg:flex flex-col gap-2 sticky top-3 self-start">
      <div className="flex items-center gap-1.5 text-muted-foreground text-[10px] font-medium uppercase tracking-wider px-1">
        <Blocks className="size-3" /> blocks — drag to insert
      </div>
      {blocks.length === 0 && (
        <p className="font-mono text-xs text-muted-foreground italic px-1">// no blocks yet</p>
      )}
      <div className="flex flex-col gap-1">
        {blocks.map(([slug, block]) => (
          <div
            key={slug}
            draggable
            onDragStart={(e) => {
              e.dataTransfer.setData(DND_BLOCK_SLUG, slug)
              e.dataTransfer.effectAllowed = "copy"
            }}
            className="flex items-center gap-1.5 rounded-md border border-border bg-card px-2 py-1.5 font-mono text-xs cursor-grab active:cursor-grabbing"
            title={`Drag to insert "${block.name}"`}
          >
            <GripVertical className="size-3 text-muted-foreground shrink-0" />
            <span className="truncate">{block.name}</span>
          </div>
        ))}
      </div>
    </aside>
  )
}
