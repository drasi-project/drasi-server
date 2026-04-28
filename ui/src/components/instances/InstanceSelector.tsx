import { useState } from "react";
import { ChevronDown, Plus, Layers, Copy, Package } from "lucide-react";
import type { InstanceInfo } from "@/api/types";

interface InstanceSelectorProps {
  instances: InstanceInfo[];
  selectedId?: string;
  onSelect: (id: string) => void;
  onCreateNew?: () => void;
  onCreateFromTemplate?: () => void;
  onClone?: () => void;
  onCreateTemplate?: () => void;
}

export default function InstanceSelector({
  instances,
  selectedId,
  onSelect,
  onCreateNew,
  onCreateFromTemplate,
  onClone,
  onCreateTemplate,
}: InstanceSelectorProps) {
  const [open, setOpen] = useState(false);
  const current = instances.find((i) => i.id === selectedId);
  const displayName = current
    ? current.id.length > 32
      ? current.id.slice(0, 32) + "…"
      : current.id
    : "No instance";

  const hasCurrentInstance = !!current;

  return (
    <div className="relative">
      <button
        onClick={() => setOpen(!open)}
        className="flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-drasi-card border border-drasi-border text-xs text-drasi-text-primary hover:border-drasi-text-secondary transition-colors"
      >
        <Layers size={12} className="text-drasi-text-secondary" />
        <span className="font-medium">{displayName}</span>
        <ChevronDown
          size={12}
          className={`text-drasi-text-secondary transition-transform ${open ? "rotate-180" : ""}`}
        />
      </button>

      {open && (
        <>
          <div
            className="fixed inset-0 z-40"
            onClick={() => setOpen(false)}
          />
          <div className="absolute top-full left-0 mt-1 w-96 bg-drasi-surface border border-drasi-border rounded-lg shadow-2xl z-50 overflow-hidden animate-fade-in">
            <div className="p-2 border-b border-drasi-border">
              <p className="text-[10px] text-drasi-text-secondary uppercase tracking-wider px-2 py-1">
                Instances
              </p>
            </div>
            <div className="max-h-48 overflow-y-auto p-1">
              {instances.map((inst) => (
                <button
                  key={inst.id}
                  onClick={() => {
                    onSelect(inst.id);
                    setOpen(false);
                  }}
                  className={`w-full flex items-center justify-between px-3 py-2 rounded-md text-left transition-colors ${
                    inst.id === selectedId
                      ? "bg-drasi-card text-drasi-text-primary"
                      : "text-drasi-text-secondary hover:bg-drasi-card hover:text-drasi-text-primary"
                  }`}
                >
                  <span className="text-xs font-mono truncate">
                    {inst.id}
                  </span>
                  <span className="text-[10px] text-drasi-text-secondary ml-2 flex-shrink-0">
                    {inst.source_count}S {inst.query_count}Q{" "}
                    {inst.reaction_count}R
                  </span>
                </button>
              ))}
            </div>
            <div className="p-1 border-t border-drasi-border space-y-0.5">
              {onCreateNew && (
              <button
                onClick={() => {
                  onCreateNew();
                  setOpen(false);
                }}
                className="w-full flex items-center gap-2 px-3 py-2 rounded-md text-xs text-drasi-text-secondary hover:bg-drasi-card hover:text-drasi-text-primary transition-colors"
              >
                <Plus size={12} />
                Create Instance
              </button>
              )}
              {onCreateFromTemplate && (
                <button
                  onClick={() => {
                    onCreateFromTemplate();
                    setOpen(false);
                  }}
                  className="w-full flex items-center gap-2 px-3 py-2 rounded-md text-xs text-drasi-text-secondary hover:bg-drasi-card hover:text-drasi-text-primary transition-colors"
                >
                  <Package size={12} />
                  Create from Solution Template
                </button>
              )}
              {hasCurrentInstance && onClone && (
                <button
                  onClick={() => {
                    onClone();
                    setOpen(false);
                  }}
                  className="w-full flex items-center gap-2 px-3 py-2 rounded-md text-xs text-drasi-text-secondary hover:bg-drasi-card hover:text-drasi-text-primary transition-colors"
                >
                  <Copy size={12} />
                  Clone Instance
                </button>
              )}
              {hasCurrentInstance && onCreateTemplate && (
                <button
                  onClick={() => {
                    onCreateTemplate();
                    setOpen(false);
                  }}
                  className="w-full flex items-center gap-2 px-3 py-2 rounded-md text-xs text-drasi-text-secondary hover:bg-drasi-card hover:text-drasi-text-primary transition-colors"
                >
                  <Package size={12} />
                  Create Solution Template
                </button>
              )}
            </div>
          </div>
        </>
      )}
    </div>
  );
}
