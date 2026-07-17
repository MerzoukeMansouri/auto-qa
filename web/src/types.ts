export interface ActionEntry {
  action: string
  assertion: string
}

export function emptyEntry(): ActionEntry {
  return { action: "", assertion: "" }
}
