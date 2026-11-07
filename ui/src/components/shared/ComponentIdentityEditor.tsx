import { useState } from "react";
import { apiErrorMessage } from "@/api/client";

interface Props {
  id: string;
  name: string;
  onRename?: (name: string) => Promise<void>;
}

export default function ComponentIdentityEditor({ id, name, onRename }: Props) {
  const [editing, setEditing] = useState(false);
  const [value, setValue] = useState(name);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  return (
    <div className="min-w-0 space-y-1">
      {editing ? (
        <form className="flex flex-wrap gap-1" onSubmit={async (event) => {
          event.preventDefault();
          if (!onRename || saving) return;
          setSaving(true);
          setError(null);
          try {
            await onRename(value);
            setEditing(false);
          } catch (failure) {
            setError(apiErrorMessage(failure));
          } finally {
            setSaving(false);
          }
        }}>
          <input aria-label="Component name" value={value} required disabled={saving}
            onChange={(event) => setValue(event.target.value)}
            className="min-w-0 flex-1 rounded border border-drasi-border bg-drasi-card p-1 text-sm text-drasi-text-primary" />
          <button type="submit" disabled={saving}
            className="rounded border border-drasi-border px-2 text-xs text-drasi-text-primary">
            {saving ? "Saving" : "Save"}
          </button>
          <button type="button" disabled={saving} onClick={() => setEditing(false)}
            className="text-xs text-drasi-text-secondary">Cancel</button>
        </form>
      ) : (
        <div className="flex items-center gap-2">
          <h2 className="min-w-0 truncate text-sm font-bold text-drasi-text-primary" title={name}>{name}</h2>
          {onRename && <button type="button" className="text-xs text-drasi-text-secondary hover:underline"
            onClick={() => { setValue(name); setError(null); setEditing(true); }}>Rename</button>}
        </div>
      )}
      <code className="block break-all text-[10px] text-drasi-text-secondary" title="Immutable component UUID">{id}</code>
      {error && <p role="alert" className="text-xs text-drasi-error">{error}</p>}
    </div>
  );
}
