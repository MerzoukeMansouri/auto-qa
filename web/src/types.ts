export interface Step {
  kind: "step"
  action: string
  assertion: string
}

export interface BlockStepRef {
  kind: "block"
  slug: string
  bindings: Record<string, string>
}

export type TestStep = Step | BlockStepRef

export function emptyStep(): Step {
  return { kind: "step", action: "", assertion: "" }
}

export interface Block {
  name: string
  steps: { action: string; assertion: string }[]
}

export interface Test {
  name: string
  steps: TestStep[]
}

export interface Param {
  name: string
  value: string
}
