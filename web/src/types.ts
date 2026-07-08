export interface ActionEntry {
  kind: string
  selector?: string
  tag?: string
  text?: string
  x?: number
  y?: number
  x2?: number
  y2?: number
  value?: string
  enter?: boolean
  combo?: string
  url?: string
  direction?: string
  magnitude?: number
  seconds?: number
  assert_kind?: string
  screenshot?: string
}

export function emptyAssert(): ActionEntry {
  return { kind: "assert", assert_kind: "visible", selector: "" }
}
