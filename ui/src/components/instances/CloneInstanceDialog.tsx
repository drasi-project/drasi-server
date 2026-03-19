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

  const totalComponents = 
    sourceComponentCounts.sources + 
    sourceComponentCounts.queries + 
    sourceComponentCounts.reactions;

  const handleClone = async () => {
    if (!newId.trim()) {
      setError("Required");
      return;
    }

    setCloneState("cloning");
    setProgress({ phase: "Creating instance", current: 0, total: totalComponents + 1 });

    try {
      // Step 1: Create the new instance
      await api.createInstance({ id: newId.trim() });
      setProgress({ phase: "Cloning sources", current: 1, total: totalComponents + 1 });

      // Step 2: Fetch and clone all sources
      const sources = await api.listSources(sourceInstanceId);
      for (let i = 0; i < sources.length; i++) {
        const src = sources[i];
        setProgress({ 
          phase: `Cloning source: ${src.id}`, 
          current: 1 + i + 1, 
          total: totalComponents + 1 
        });
        
        // Get full source config
        const fullSource = await api.getSource(src.id, sourceInstanceId);
        
        // Create in new instance with autoStart: false
        await api.createSource({
          kind: fullSource.kind,
          id: fullSource.id,
          autoStart: false,
          ...fullSource.properties,
        }, newId);
      }

      // Step 3: Fetch and clone all queries
      const queries = await api.listQueries(sourceInstanceId);
      const queriesOffset = 1 + sources.length;
      for (let i = 0; i < queries.length; i++) {
        const q = queries[i];
        setProgress({ 
          phase: `Cloning query: ${q.id}`, 
          current: queriesOffset + i + 1, 
          total: totalComponents + 1 
        });
        
        // Create query in new instance - sources is array of QuerySourceSubscription
        await api.createQuery({
          id: q.id,
          query: q.query ?? "",
          queryLanguage: q.queryLanguage ?? "Cypher",
          sources: q.sources ?? [],
          autoStart: false,
        }, newId);
      }

      // Step 4: Fetch and clone all reactions
      const reactions = await api.listReactions(sourceInstanceId);
      const reactionsOffset = queriesOffset + queries.length;
      for (let i = 0; i < reactions.length; i++) {
        const r = reactions[i];
        setProgress({ 
          phase: `Cloning reaction: ${r.id}`, 
          current: reactionsOffset + i + 1, 
          total: totalComponents + 1 
        });
        
        // Get full reaction config
        const fullReaction = await api.getReaction(r.id, sourceInstanceId);
        
        // Create in new instance with autoStart: false
        await api.createReaction({
          kind: fullReaction.kind,
          id: fullReaction.id,
          queries: fullReaction.queries ?? [],
          autoStart: false,
          ...fullReaction.properties,
        }, newId);
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
