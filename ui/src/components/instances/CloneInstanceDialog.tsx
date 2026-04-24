import { useState } from "react";
import { Loader2, Check, AlertCircle, Copy, Database, Search, Zap } from "lucide-react";
import FormField from "@/components/create/FormField";
import * as api from "@/api/client";

interface CloneInstanceDialogProps {
  sourceInstanceId: string;
  sourceComponentCounts: {
    sources: number;
    queries: number;
    reactions: number;
  };
  onSuccess: (newInstanceId: string) => void;
  onCancel: () => void;
}

type CloneState = "form" | "cloning" | "success" | "error";

interface CloneProgress {
  phase: string;
  current: number;
  total: number;
}

export default function CloneInstanceDialog({
  sourceInstanceId,
  sourceComponentCounts,
  onSuccess,
  onCancel,
}: CloneInstanceDialogProps) {
  const [newId, setNewId] = useState(`${sourceInstanceId}-clone`);
  const [error, setError] = useState("");
  const [cloneState, setCloneState] = useState<CloneState>("form");
  const [progress, setProgress] = useState<CloneProgress>({ phase: "", current: 0, total: 0 });
  const [cloneError, setCloneError] = useState<string | null>(null);

  const handleClone = async () => {
    if (!newId.trim()) {
      setError("Required");
      return;
    }

    setCloneState("cloning");
    setProgress({ phase: "Creating instance", current: 1, total: 2 });

    try {
      // Step 1: Create the new empty instance
      await api.createInstance({ id: newId.trim() });

      // Step 2: Clone all components via server-side endpoint
      setProgress({ phase: "Cloning components", current: 2, total: 2 });
      const result = await api.cloneInstance(newId.trim(), sourceInstanceId);
      if (!result.success) {
        throw new Error(result.errors.join(", "));
      }

      setCloneState("success");
      
      // Auto-close after brief success display
      setTimeout(() => {
        onSuccess(newId);
      }, 1500);

    } catch (err) {
      setCloneState("error");
      setCloneError(err instanceof Error ? err.message : "Failed to clone instance");
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm animate-fade-in">
      <div className="bg-drasi-surface border border-drasi-border rounded-2xl p-6 max-w-md w-full mx-4 shadow-2xl">
        {cloneState === "form" && (
          <>
            <div className="flex items-center gap-3 mb-4">
              <div className="w-10 h-10 rounded-xl bg-drasi-accent/20 flex items-center justify-center">
                <Copy size={20} className="text-drasi-accent" />
              </div>
              <div>
                <h2 className="text-lg font-bold text-drasi-text-primary">
                  Clone Instance
                </h2>
                <p className="text-xs text-drasi-text-secondary">
                  Create a copy of <span className="font-mono">{sourceInstanceId}</span>
                </p>
              </div>
            </div>

            <div className="mb-4 p-3 rounded-lg bg-drasi-card border border-drasi-border">
              <p className="text-xs text-drasi-text-secondary mb-2">Components to clone:</p>
              <div className="flex items-center gap-4 text-sm">
                <span className="flex items-center gap-1.5">
                  <Database size={14} className="text-drasi-source" />
                  <span className="text-drasi-text-primary">{sourceComponentCounts.sources}</span>
                  <span className="text-drasi-text-secondary">sources</span>
                </span>
                <span className="flex items-center gap-1.5">
                  <Search size={14} className="text-drasi-query" />
                  <span className="text-drasi-text-primary">{sourceComponentCounts.queries}</span>
                  <span className="text-drasi-text-secondary">queries</span>
                </span>
                <span className="flex items-center gap-1.5">
                  <Zap size={14} className="text-drasi-reaction" />
                  <span className="text-drasi-text-primary">{sourceComponentCounts.reactions}</span>
                  <span className="text-drasi-text-secondary">reactions</span>
                </span>
              </div>
            </div>

            <div className="space-y-4">
              <FormField
                label="New Instance ID"
                field="newId"
                value={newId}
                onChange={(_, v) => {
                  setNewId(String(v));
                  setError("");
                }}
                error={error}
                required
                placeholder="my-instance-clone"
              />
              <p className="text-[10px] text-drasi-text-secondary">
                All components will be copied with autoStart disabled.
              </p>
            </div>

            <div className="flex justify-end gap-2 mt-6">
              <button onClick={onCancel} className="action-btn-ghost">
                Cancel
              </button>
              <button onClick={handleClone} className="action-btn-primary">
                Clone
              </button>
            </div>
          </>
        )}

        {cloneState === "cloning" && (
          <div className="text-center py-4">
            <Loader2 size={40} className="animate-spin text-drasi-accent mx-auto mb-4" />
            <p className="text-sm font-medium text-drasi-text-primary mb-2">
              {progress.phase}
            </p>
            <div className="w-full bg-drasi-card rounded-full h-2 mb-2">
              <div 
                className="bg-drasi-accent h-2 rounded-full transition-all duration-300"
                style={{ width: `${(progress.current / progress.total) * 100}%` }}
              />
            </div>
            <p className="text-xs text-drasi-text-secondary">
              {progress.current} of {progress.total}
            </p>
          </div>
        )}

        {cloneState === "success" && (
          <div className="text-center py-4">
            <div className="w-16 h-16 mx-auto mb-4 rounded-full bg-drasi-running/10 flex items-center justify-center">
              <Check size={32} className="text-drasi-running" />
            </div>
            <p className="text-lg font-medium text-drasi-text-primary">
              Instance Cloned!
            </p>
            <p className="text-sm text-drasi-text-secondary mt-1">
              Switching to <span className="font-mono">{newId}</span>
            </p>
          </div>
        )}

        {cloneState === "error" && (
          <div className="text-center py-4">
            <div className="w-16 h-16 mx-auto mb-4 rounded-full bg-drasi-error/10 flex items-center justify-center">
              <AlertCircle size={32} className="text-drasi-error" />
            </div>
            <p className="text-lg font-medium text-drasi-text-primary mb-2">
              Clone Failed
            </p>
            <p className="text-sm text-drasi-error mb-4">
              {cloneError}
            </p>
            <div className="flex justify-center gap-2">
              <button onClick={onCancel} className="action-btn-ghost">
                Close
              </button>
              <button 
                onClick={() => {
                  setCloneState("form");
                  setCloneError(null);
                }} 
                className="action-btn-primary"
              >
                Try Again
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
