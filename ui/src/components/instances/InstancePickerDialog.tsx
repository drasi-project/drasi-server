import { Layers, Plus } from "lucide-react";
import type { InstanceInfo } from "@/api/types";

interface InstancePickerDialogProps {
  instances: InstanceInfo[];
  missingId: string;
  onSelect: (id: string) => void;
  onCreateNew: () => void;
}

export default function InstancePickerDialog({
  instances,
  missingId,
  onSelect,
  onCreateNew,
}: InstancePickerDialogProps) {
  const accentColor = "#8b5cf6";

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm animate-fade-in">
      <div className="bg-drasi-surface border border-drasi-border rounded-2xl p-6 max-w-lg w-full mx-4 shadow-2xl">
        <h2 className="text-lg font-bold text-drasi-text-primary mb-1 text-center">
          Instance not found
        </h2>
        <p className="text-sm text-drasi-text-secondary text-center mb-5">
          <span className="font-mono font-semibold text-drasi-text-primary">{missingId}</span>{" "}
          does not exist. Select an instance or create a new one.
        </p>
        <div className="grid grid-cols-3 gap-3">
          {instances.map((inst) => (
            <button
              key={inst.id}
              onClick={() => onSelect(inst.id)}
              className="type-card"
            >
              <div
                className="p-2.5 rounded-xl"
                style={{ backgroundColor: `${accentColor}20` }}
              >
                <Layers size={24} style={{ color: accentColor }} />
              </div>
              <span className="text-sm font-semibold text-drasi-text-primary truncate w-full text-center">
                {inst.id}
              </span>
              <span className="text-[10px] text-drasi-text-secondary text-center leading-tight">
                {inst.source_count}S · {inst.query_count}Q · {inst.reaction_count}R
              </span>
            </button>
          ))}
          <button onClick={onCreateNew} className="type-card">
            <div
              className="p-2.5 rounded-xl"
              style={{ backgroundColor: "#06b6d420" }}
            >
              <Plus size={24} style={{ color: "#06b6d4" }} />
            </div>
            <span className="text-sm font-semibold text-drasi-text-primary">
              Create New
            </span>
            <span className="text-[10px] text-drasi-text-secondary text-center leading-tight">
              Create a new instance
            </span>
          </button>
        </div>
      </div>
    </div>
  );
}
