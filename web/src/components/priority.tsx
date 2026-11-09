import type { NotePriority } from "../rpc/types/NotePriority";

// Typed against the Postgres enum: adding a value in a migration makes this
// object incomplete, and `just check` fails until it is handled here.
const LABELS: Record<NotePriority, string> = {
  low: "Low",
  normal: "Normal",
  high: "High",
};

export function PrioritySelect({ value, onChange }: { value: NotePriority; onChange: (p: NotePriority) => void }) {
  return (
    <select value={value} onChange={(e) => onChange(e.target.value as NotePriority)} aria-label="Priority">
      {(Object.keys(LABELS) as NotePriority[]).map((p) => (
        <option key={p} value={p}>
          {LABELS[p]}
        </option>
      ))}
    </select>
  );
}
