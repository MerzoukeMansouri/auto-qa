import { Button } from "@/components/ui/button"
import type { Test } from "@/types"
import { FileCode2, Blocks, Tag, Plus, Trash2, Circle, History } from "lucide-react"

export type View = "tests" | "blocks" | "params"

export function Sidebar({
  view,
  setView,
  tests,
  currentSlug,
  dirty,
  hasBuffer,
  onOpenTest,
  onNewTest,
  onDeleteTest,
  onOpenLastRun,
  blocksCount,
  paramsCount,
}: {
  view: View
  setView: (v: View) => void
  tests: [string, Test][]
  currentSlug: string | null
  dirty: boolean
  hasBuffer: boolean
  onOpenTest: (slug: string) => void
  onNewTest: () => void
  onDeleteTest: (slug: string) => void
  onOpenLastRun: () => void
  blocksCount: number
  paramsCount: number
}) {
  return (
    <nav className="w-64 shrink-0 h-screen sticky top-0 border-r border-border flex flex-col font-mono text-xs bg-background">
      <div className="px-4 py-3 border-b border-border">
        <h1 className="text-[15px] font-semibold">autoqa</h1>
        <p className="text-muted-foreground text-[10px] mt-0.5">test / block / debug</p>
      </div>

      <div className="flex-1 overflow-y-auto py-3 flex flex-col gap-4">
        <section>
          <button
            onClick={onOpenLastRun}
            className="w-full flex items-center gap-1.5 px-4 py-1.5 text-muted-foreground text-[10px] font-medium uppercase tracking-wider hover:bg-accent/50 hover:text-accent-foreground"
            title="Load the most recent `autoqa run` session into the editor"
          >
            <History className="size-3" /> last run test
          </button>
        </section>

        <section>
          <div className="flex items-center justify-between px-4 mb-1.5">
            <div className="flex items-center gap-1.5 text-muted-foreground text-[10px] font-medium uppercase tracking-wider">
              <FileCode2 className="size-3" /> tests
            </div>
            <Button variant="ghost" size="icon" className="size-5" onClick={onNewTest} aria-label="New test" title="New test">
              <Plus className="size-3" />
            </Button>
          </div>
          <ul>
            {currentSlug === null && (
              <li>
                <button
                  onClick={() => setView("tests")}
                  className={
                    "w-full flex items-center gap-1.5 px-4 py-1.5 text-left truncate " +
                    (view === "tests" ? "bg-accent text-accent-foreground" : "hover:bg-accent/50")
                  }
                >
                  <span className="italic text-muted-foreground truncate">untitled</span>
                  {hasBuffer && dirty && (
                    <Circle className="size-1.5 shrink-0 fill-primary text-primary" aria-label="unsaved changes" />
                  )}
                </button>
              </li>
            )}
            {tests.map(([slug, t]) => (
              <li key={slug} className="group flex items-center">
                <button
                  onClick={() => onOpenTest(slug)}
                  className={
                    "flex-1 min-w-0 flex items-center gap-1.5 px-4 py-1.5 text-left truncate " +
                    (view === "tests" && slug === currentSlug
                      ? "bg-accent text-accent-foreground"
                      : "hover:bg-accent/50")
                  }
                >
                  <span className="truncate">{t.name}</span>
                  {slug === currentSlug && dirty && (
                    <Circle className="size-1.5 shrink-0 fill-primary text-primary" aria-label="unsaved changes" />
                  )}
                </button>
                <Button
                  variant="ghost"
                  size="icon"
                  className="size-6 mr-2 opacity-0 group-hover:opacity-100 shrink-0"
                  onClick={() => onDeleteTest(slug)}
                  aria-label={`Delete test ${t.name}`}
                  title="Delete test"
                >
                  <Trash2 className="size-3" />
                </Button>
              </li>
            ))}
            {tests.length === 0 && currentSlug === null && !hasBuffer && (
              <li className="px-4 py-1 text-muted-foreground italic">no tests yet</li>
            )}
          </ul>
        </section>

        <section>
          <button
            onClick={() => setView("blocks")}
            className={
              "w-full flex items-center justify-between px-4 py-1.5 " +
              (view === "blocks" ? "bg-accent text-accent-foreground" : "hover:bg-accent/50")
            }
          >
            <span className="flex items-center gap-1.5 text-muted-foreground text-[10px] font-medium uppercase tracking-wider">
              <Blocks className="size-3" /> blocks
            </span>
            <span className="text-muted-foreground tabular-nums">{blocksCount}</span>
          </button>
        </section>

        <section>
          <button
            onClick={() => setView("params")}
            className={
              "w-full flex items-center justify-between px-4 py-1.5 " +
              (view === "params" ? "bg-accent text-accent-foreground" : "hover:bg-accent/50")
            }
          >
            <span className="flex items-center gap-1.5 text-muted-foreground text-[10px] font-medium uppercase tracking-wider">
              <Tag className="size-3" /> params
            </span>
            <span className="text-muted-foreground tabular-nums">{paramsCount}</span>
          </button>
        </section>
      </div>
    </nav>
  )
}
