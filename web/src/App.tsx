import { useEffect, useRef, useState } from "react"
import { Sidebar, type View } from "@/components/Sidebar"
import { StepsView } from "@/components/StepsView"
import { BlocksPalette } from "@/components/BlocksPalette"
import { BlocksView, slugify } from "@/components/BlocksView"
import { ParamsView } from "@/components/ParamsView"
import type { TestStep, Block, Param, Test } from "@/types"
import { emptyStep } from "@/types"
import { reorder } from "@/lib/dnd"

export default function App() {
  const [entries, setEntries] = useState<TestStep[]>([])
  const [snapshot, setSnapshot] = useState<TestStep[]>([])
  const [currentSlug, setCurrentSlug] = useState<string | null>(null)
  const [tests, setTests] = useState<[string, Test][]>([])
  const [blocks, setBlocks] = useState<[string, Block][]>([])
  const [params, setParams] = useState<Param[]>([])
  const [toast, setToast] = useState<{ text: string; variant: "info" | "pass" | "fail" } | null>(null)
  const [running, setRunning] = useState(false)
  const [chatInput, setChatInput] = useState("")
  const [chatBusy, setChatBusy] = useState(false)
  const [view, setView] = useState<View>("tests")
  const [selected, setSelected] = useState<Set<number>>(new Set())
  const saveTimer = useRef<number | undefined>(undefined)

  useEffect(() => {
    fetch("/api/actions")
      .then((r) => r.json())
      .then((steps: TestStep[]) => {
        setEntries(steps)
        setSnapshot(steps)
      })
    refreshBlocks()
    refreshTests()
    fetch("/api/params")
      .then((r) => r.json())
      .then(setParams)
  }, [])

  function refreshBlocks(): Promise<void> {
    return fetch("/api/blocks")
      .then((r) => r.json())
      .then(setBlocks)
  }

  function refreshTests(): Promise<void> {
    return fetch("/api/tests")
      .then((r) => r.json())
      .then(setTests)
  }

  const dirty = JSON.stringify(entries) !== JSON.stringify(snapshot)
  const currentTestName = currentSlug
    ? (tests.find(([slug]) => slug === currentSlug)?.[1].name ?? currentSlug)
    : "untitled"

  function scheduleSave(next: TestStep[]) {
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

  function updateAt(i: number, next: TestStep) {
    const copy = entries.slice()
    copy[i] = next
    scheduleSave(copy)
  }

  function deleteAt(i: number) {
    scheduleSave(entries.filter((_, idx) => idx !== i))
    setSelected((s) => {
      const next = new Set(Array.from(s).filter((idx) => idx !== i).map((idx) => (idx > i ? idx - 1 : idx)))
      return next
    })
  }

  function addEntry() {
    scheduleSave([...entries, emptyStep()])
  }

  function moveStep(from: number, to: number) {
    scheduleSave(reorder(entries, from, to))
  }

  function insertBlockAt(gap: number, slug: string) {
    const copy = entries.slice()
    copy.splice(gap, 0, { kind: "block", slug, bindings: {} })
    scheduleSave(copy)
  }

  function toggleSelect(i: number) {
    setSelected((s) => {
      const next = new Set(s)
      if (next.has(i)) next.delete(i)
      else next.add(i)
      return next
    })
  }

  async function saveSelectionAsBlock() {
    const indices = Array.from(selected).sort((a, b) => a - b)
    if (indices.length === 0) return
    const name = window.prompt("Block name?")
    if (!name || !name.trim()) return
    const slug = slugify(name)
    const steps = indices
      .map((i) => entries[i])
      .filter((e): e is Extract<TestStep, { kind: "step" }> => e.kind === "step")
      .map(({ action, assertion }) => ({ action, assertion }))
    const block: Block = { name: name.trim(), steps }
    await fetch(`/api/blocks/${slug}`, {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(block),
    })
    const insertPos = indices[0]
    const next = entries.filter((_, i) => !selected.has(i))
    next.splice(insertPos, 0, { kind: "block", slug, bindings: {} })
    scheduleSave(next)
    setSelected(new Set())
    await refreshBlocks()
    showToast(`saved block "${name.trim()}"`, "pass")
  }

  async function saveNow(next?: TestStep[]) {
    window.clearTimeout(saveTimer.current)
    const body = next ?? entries
    await fetch("/api/actions", {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    })
  }

  function showToast(text: string, variant: "info" | "pass" | "fail") {
    setToast({ text, variant })
    setTimeout(() => setToast(null), 5000)
  }

  async function openTest(slug: string) {
    const res = await fetch(`/api/tests/${slug}/open`, { method: "POST" })
    const steps: TestStep[] = await res.json()
    setEntries(steps)
    setSnapshot(steps)
    setCurrentSlug(slug)
    setSelected(new Set())
    setView("tests")
  }

  async function openLastRun() {
    const res = await fetch("/api/last-run/open", { method: "POST" })
    if (!res.ok) {
      showToast(await res.text(), "fail")
      return
    }
    const steps: TestStep[] = await res.json()
    setEntries(steps)
    // Loaded as an untitled buffer, not tied to any saved test — matches
    // "New test" semantics (snapshot equals the loaded content, so it
    // isn't flagged dirty until actually edited further; "Save as" is how
    // you'd turn it into a named test).
    setSnapshot(steps)
    setCurrentSlug(null)
    setSelected(new Set())
    setView("tests")
    showToast("loaded last run", "info")
  }

  async function newTest() {
    await saveNow([])
    setEntries([])
    setSnapshot([])
    setCurrentSlug(null)
    setSelected(new Set())
    setView("tests")
  }

  async function saveTest() {
    if (currentSlug) {
      await persistTest(currentSlug, tests.find(([s]) => s === currentSlug)?.[1].name ?? currentSlug)
    } else {
      await saveTestAs()
    }
  }

  async function saveTestAs() {
    const name = window.prompt("Test name?", currentSlug ? currentTestName : "")
    if (!name || !name.trim()) return
    await persistTest(slugify(name), name.trim())
  }

  async function persistTest(slug: string, name: string) {
    await saveNow()
    const test: Test = { name, steps: entries }
    await fetch(`/api/tests/${slug}`, {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(test),
    })
    setCurrentSlug(slug)
    setSnapshot(entries)
    await refreshTests()
    showToast(`saved test "${name}"`, "pass")
  }

  async function deleteTest(slug: string) {
    const name = tests.find(([s]) => s === slug)?.[1].name ?? slug
    if (!window.confirm(`Delete test "${name}"?`)) return
    await fetch(`/api/tests/${slug}`, { method: "DELETE" })
    if (slug === currentSlug) {
      setCurrentSlug(null)
      setSnapshot(entries)
    }
    await refreshTests()
    showToast(`deleted test "${name}"`, "info")
  }

  async function validate() {
    await saveNow()
    const res = await fetch("/api/validate", { method: "POST" })
    const data = await res.json()
    showToast(`wrote ${data.path}`, "info")
  }

  async function pauseAt(i: number) {
    await saveNow()
    const res = await fetch(`/api/pause/${i}`, { method: "POST" })
    if (res.ok) {
      showToast(`opening inspector at step ${i + 1}…`, "info")
    } else {
      showToast(`could not open inspector at step ${i + 1}`, "fail")
    }
  }

  async function submitChat() {
    if (!chatInput.trim() || chatBusy) return
    await saveNow()
    setChatBusy(true)
    try {
      const res = await fetch("/api/chat", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ instruction: chatInput }),
      })
      if (res.ok) {
        setEntries(await res.json())
        setChatInput("")
        showToast("steps updated", "pass")
      } else {
        showToast(await res.text(), "fail")
      }
    } finally {
      setChatBusy(false)
    }
  }

  async function runTest() {
    await saveNow()
    setRunning(true)
    try {
      const res = await fetch("/api/run", { method: "POST" })
      const data = await res.json()
      if (data.passed) {
        showToast("test passed", "pass")
      } else {
        console.error(data.output)
        showToast("test failed — see browser console for output", "fail")
      }
    } finally {
      setRunning(false)
    }
  }

  return (
    <div className="min-h-screen flex">
      <Sidebar
        view={view}
        setView={setView}
        tests={tests}
        currentSlug={currentSlug}
        dirty={dirty}
        hasBuffer={entries.length > 0}
        onOpenTest={openTest}
        onNewTest={newTest}
        onDeleteTest={deleteTest}
        onOpenLastRun={openLastRun}
        blocksCount={blocks.length}
        paramsCount={params.length}
      />

      <main className="flex-1 min-w-0 p-6 flex gap-6">
        {view === "tests" && (
          <>
            <StepsView
              entries={entries}
              blocks={blocks}
              params={params}
              testName={currentTestName}
              dirty={dirty}
              selected={selected}
              running={running}
              chatInput={chatInput}
              setChatInput={setChatInput}
              chatBusy={chatBusy}
              updateAt={updateAt}
              deleteAt={deleteAt}
              addEntry={addEntry}
              moveStep={moveStep}
              insertBlockAt={insertBlockAt}
              toggleSelect={toggleSelect}
              saveSelectionAsBlock={saveSelectionAsBlock}
              pauseAt={pauseAt}
              submitChat={submitChat}
              validate={validate}
              runTest={runTest}
              saveTest={saveTest}
              saveTestAs={saveTestAs}
            />
            <BlocksPalette blocks={blocks} />
          </>
        )}

        {view === "blocks" && (
          <div className="flex-1 min-w-0">
            <BlocksView blocks={blocks} refreshBlocks={refreshBlocks} showToast={showToast} />
          </div>
        )}

        {view === "params" && (
          <div className="flex-1 min-w-0">
            <ParamsView params={params} setParams={setParams} />
          </div>
        )}
      </main>

      {toast && (
        <div
          className={
            "fixed bottom-4 right-4 bg-card border-y border-r border-border rounded-md px-4 py-2 shadow-lg text-sm font-mono border-l-2 " +
            (toast.variant === "pass"
              ? "border-l-success"
              : toast.variant === "fail"
                ? "border-l-destructive"
                : "border-l-primary")
          }
        >
          {toast.text}
        </div>
      )}
    </div>
  )
}
