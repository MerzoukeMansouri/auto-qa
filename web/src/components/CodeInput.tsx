import { Input } from "@/components/ui/input"

export function CodeInput({
  value,
  placeholder,
  onChange,
  muted,
}: {
  value: string
  placeholder?: string
  onChange: (v: string) => void
  muted?: boolean
}) {
  return (
    <Input
      className={
        "h-auto border-transparent bg-transparent px-1 py-0.5 font-mono text-xs shadow-none focus-visible:border-input focus-visible:bg-input/20" +
        (muted ? " text-muted-foreground" : "")
      }
      value={value}
      placeholder={placeholder}
      onChange={(e) => onChange(e.target.value)}
    />
  )
}
