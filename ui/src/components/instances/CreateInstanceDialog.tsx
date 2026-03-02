import { useState } from "react";
import FormField from "@/components/create/FormField";

interface CreateInstanceDialogProps {
  onSave: (data: {
    id: string;
    persistIndex?: boolean;
    defaultPriorityQueueCapacity?: number;
    defaultDispatchBufferCapacity?: number;
  }) => Promise<void>;
  onCancel: () => void;
  /** Pre-fill the instance ID field (e.g. from a URL param that wasn't found) */
  initialId?: string;
}

export default function CreateInstanceDialog({
  onSave,
  onCancel,
  initialId,
}: CreateInstanceDialogProps) {
  const [id, setId] = useState(initialId ?? "");
  const [persistIndex, setPersistIndex] = useState(false);
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);

  const handleSave = async () => {
    if (!id.trim()) {
      setError("Required");
      return;
    }
    setSaving(true);
    try {
      await onSave({ 
        id: id.trim(), 
        persistIndex,
      });
    } catch {
      setError("Failed to create instance");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm animate-fade-in">
      <div className="bg-drasi-surface border border-drasi-border rounded-2xl p-6 max-w-md w-full mx-4 shadow-2xl">
        <h2 className="text-lg font-bold text-drasi-text-primary mb-4">
          Create Instance
        </h2>
        <div className="space-y-4">
          <FormField
            label="Instance ID"
            field="id"
            value={id}
            onChange={(_, v) => {
              setId(String(v));
              setError("");
            }}
            error={error}
            required
            placeholder="my-instance"
          />
          <FormField
            label="Persist Index (RocksDB)"
            field="persistIndex"
            value={persistIndex}
            onChange={(_, v) => setPersistIndex(Boolean(v))}
            type="toggle"
            helpText="Use RocksDB for persistent query indexes"
          />
        </div>

        <div className="flex justify-end gap-2 mt-6">
          <button onClick={onCancel} className="action-btn-ghost" disabled={saving}>
            Cancel
          </button>
          <button onClick={handleSave} className="action-btn-primary" disabled={saving}>
            {saving ? "Creating…" : "Create"}
          </button>
        </div>
      </div>
    </div>
  );
}
