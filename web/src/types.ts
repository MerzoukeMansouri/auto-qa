export interface ActionEntry {
  kind: string
  selector?: string
  value?: string
  assert_kind?: string
}

export function emptyAssert(): ActionEntry {
  return { kind: "assert", assert_kind: "visible", selector: "" }
}
