// Native HTML5 drag-and-drop helpers shared by the step list (App/StepsView)
// and the block step list (BlocksView). No dnd library — just dataTransfer
// mime types + a plain array-move function.

/** Custom mime type used when dragging a step/block-step row to reorder it. */
export const DND_REORDER = "application/x-autoqa-reorder-index"
/** Custom mime type used when dragging a block from the palette to insert it. */
export const DND_BLOCK_SLUG = "application/x-autoqa-block-slug"

/** Move the item at `from` so it ends up at gap index `to` (0..list.length, "before item to"). */
export function reorder<T>(list: T[], from: number, to: number): T[] {
  const copy = list.slice()
  const [item] = copy.splice(from, 1)
  const insertAt = to > from ? to - 1 : to
  copy.splice(insertAt, 0, item)
  return copy
}

/** Which gap (0..length) the pointer is closer to, given the row's bounding box and index. */
export function gapForRow(e: React.DragEvent, rowIndex: number): number {
  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
  const before = e.clientY < rect.top + rect.height / 2
  return before ? rowIndex : rowIndex + 1
}
