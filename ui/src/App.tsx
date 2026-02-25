import { useState, useCallback, useEffect } from "react";
import AppLayout from "@/layouts/AppLayout";
import FlowCanvas from "@/components/canvas/FlowCanvas";
import InspectorPanel from "@/components/inspector/InspectorPanel";
import TypeSelector, {
  type SelectableType,
} from "@/components/create/TypeSelector";
import CreatePanel from "@/components/create/CreatePanel";
import SourceForm from "@/components/create/SourceForms";
import QueryForm from "@/components/create/QueryForm";
import ReactionForm from "@/components/create/ReactionForms";
import EventPanel, { type EventEntry } from "@/components/events/EventPanel";
import InstanceSelector from "@/components/instances/InstanceSelector";
import InstancePickerDialog from "@/components/instances/InstancePickerDialog";
import CreateInstanceDialog from "@/components/instances/CreateInstanceDialog";
import SolutionDeployDialog from "@/components/solutions/SolutionDeployDialog";
import { useSources, useQueries, useReactions } from "@/hooks/useApi";
import { useInstances } from "@/hooks/useInstances";
import { useDraft } from "@/hooks/useDraft";
import { useTheme } from "@/hooks/useTheme";
import type { PipelineData } from "@/utils/graph";
import type { ComponentStatus, ComponentType } from "@/utils/colors";
import type {
  CreateSourceRequest,
  CreateQueryRequest,
  CreateReactionRequest,
  SourceKind,
  ReactionKind,
} from "@/api/types";

type CreateStep = "component" | "source-kind" | "reaction-kind" | null;

interface SelectedComponent {
  id: string;
  type: ComponentType;
}

// Map TypeSelector reaction kinds to API kinds
function reactionApiKind(selectorKind: string): ReactionKind {
  const map: Record<string, ReactionKind> = {
    "http-reaction": "http",
    "grpc-reaction": "grpc",
    "platform-reaction": "platform",
  };
  return (map[selectorKind] ?? selectorKind) as ReactionKind;
}

export default function App() {
  const { theme, toggleTheme } = useTheme();

  // Instance management
  const {
    instances,
    selectedId: selectedInstanceId,
    setSelectedId: setSelectedInstanceId,
    create: createInstanceApi,
    refresh: refreshInstances,
    requestedNotFound,
  } = useInstances();
  const [showCreateInstance, setShowCreateInstance] = useState(false);
  const [createInstancePrefilledId, setCreateInstancePrefilledId] = useState<string | undefined>(undefined);

  // Component hooks - scoped to selected instance
  const {
    sources,
    refresh: refreshSources,
    create: createSourceApi,
    start: startSource,
    stop: stopSource,
    remove: removeSource,
  } = useSources(selectedInstanceId);
  const {
    queries,
    refresh: refreshQueries,
    create: createQueryApi,
    start: startQuery,
    stop: stopQuery,
    remove: removeQuery,
  } = useQueries(selectedInstanceId);
  const {
    reactions,
    refresh: refreshReactions,
    create: createReactionApi,
    start: startReaction,
    stop: stopReaction,
    remove: removeReaction,
  } = useReactions(selectedInstanceId);

  // Draft store — local edits until Save
  const { draft, startDraft, updateField, isValid, setSaving, discard } =
    useDraft();

  const [selected, setSelected] = useState<SelectedComponent | null>(null);
  const [createStep, setCreateStep] = useState<CreateStep>(null);
  const [events, setEvents] = useState<EventEntry[]>([]);
  const [activityOpen, setActivityOpen] = useState(false);
  const [connected, setConnected] = useState(false);

  // Solution deploy state
  const [deployTemplateId, setDeployTemplateId] = useState<string | undefined>(undefined);
  const [deployUploadedYaml, setDeployUploadedYaml] = useState<string | undefined>(undefined);

  // Check server connectivity
  useEffect(() => {
    const check = async () => {
      try {
        const res = await fetch("/health");
        setConnected(res.ok);
      } catch {
        setConnected(false);
      }
    };
    check();
    const interval = setInterval(check, 5000);
    return () => clearInterval(interval);
  }, []);

  // Build pipeline data for the canvas
  const pipelineData: PipelineData = {
    sources: sources.map((s) => ({
      id: s.id,
      kind: s.kind,
      status: s.status,
      autoStart: s.autoStart,
      properties: s.properties,
      instanceId: selectedInstanceId,
      error: s.error,
    })),
    queries: queries.map((q) => ({
      id: q.id,
      status: q.status ?? "Stopped",
      sourceIds: q.sources.map((s) => s.sourceId),
      query: q.query,
      queryLanguage: q.queryLanguage,
      error: q.error,
      instanceId: selectedInstanceId,
    })),
    reactions: reactions.map((r) => ({
      id: r.id,
      kind: r.kind,
      status: r.status,
      queryIds: r.queries,
      properties: r.properties,
      error: r.error,
    })),
  };

  // Generate a unique ID - fallback for browsers without crypto.randomUUID
  const generateId = useCallback(() => {
    if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
      return crypto.randomUUID();
    }
    // Fallback: simple unique ID generator
    return `${Date.now()}-${Math.random().toString(36).substring(2, 11)}`;
  }, []);

  const pushEvent = useCallback(
    (message: string, type: EventEntry["type"] = "info") => {
      setEvents((prev) => [
        {
          id: generateId(),
          timestamp: new Date().toISOString(),
          message,
          type,
        },
        ...prev.slice(0, 49),
      ]);
    },
    [generateId],
  );

  const handleNodeClick = useCallback((id: string, type: string) => {
    setSelected({ id, type: type as ComponentType });
  }, []);

  // TypeSelector flow: component → kind → open CreatePanel with draft
  const handleCreateSelect = useCallback(
    (type: SelectableType) => {
      if (type === "source") {
        setCreateStep("source-kind");
        return;
      }
      if (type === "reaction") {
        setCreateStep("reaction-kind");
        return;
      }
      if (type === "query") {
        // Query has no sub-kinds — open form directly
        startDraft("query", "query");
        setCreateStep(null);
        return;
      }
      // A specific source or reaction kind was selected
      const sourceKinds = ["mock", "http", "grpc", "postgres", "platform"];
      if (sourceKinds.includes(type)) {
        startDraft("source", type);
      } else {
        // Reaction kinds — keep the selector kind for the form, map on save
        startDraft("reaction", type);
      }
      setCreateStep(null);
    },
    [startDraft],
  );

  // Save draft to server
  const handleSaveDraft = useCallback(async () => {
    if (!draft || !isValid()) return;

    setSaving(true);
    try {
      const f = draft.fields;
      if (draft.componentType === "source") {
        const req: CreateSourceRequest = {
          kind: draft.kind as SourceKind,
          id: String(f.id ?? ""),
          autoStart: Boolean(f.autoStart),
          ...f,
        };
        await createSourceApi(req);
        pushEvent(`Created source: ${req.id}`, "success");
      } else if (draft.componentType === "query") {
        const sourcesArr = (f.sources as string[]) ?? [];
        const req: CreateQueryRequest = {
          id: String(f.id ?? ""),
          query: String(f.query ?? ""),
          queryLanguage: String(f.queryLanguage ?? "Cypher"),
          sources: sourcesArr.map((sid) => ({ sourceId: sid })),
          autoStart: Boolean(f.autoStart),
        };
        await createQueryApi(req);
        pushEvent(`Created query: ${req.id}`, "success");
      } else if (draft.componentType === "reaction") {
        const apiKind = reactionApiKind(draft.kind);
        const { id: _id, queries: _q, autoStart: _a, kind: _k, ...rest } = f;
        const req: CreateReactionRequest = {
          kind: apiKind,
          id: String(f.id ?? ""),
          queries: (f.queries as string[]) ?? [],
          autoStart: Boolean(f.autoStart),
          ...rest,
        };
        await createReactionApi(req);
        pushEvent(`Created reaction: ${req.id}`, "success");
      }
      discard();
      // Refresh all to show the new component on canvas
      refreshSources();
      refreshQueries();
      refreshReactions();
    } catch (err) {
      pushEvent(
        `Failed to create: ${err instanceof Error ? err.message : "Unknown error"}`,
        "error",
      );
    } finally {
      setSaving(false);
    }
  }, [
    draft,
    isValid,
    setSaving,
    createSourceApi,
    createQueryApi,
    createReactionApi,
    discard,
    pushEvent,
    refreshSources,
    refreshQueries,
    refreshReactions,
  ]);

  // Create instance handler
  const handleCreateInstance = useCallback(
    async (data: {
      id: string;
      persistIndex?: boolean;
    }) => {
      try {
        await createInstanceApi(data);
        pushEvent(`Created instance: ${data.id}`, "success");
        setShowCreateInstance(false);
        setCreateInstancePrefilledId(undefined);
      } catch (err) {
        pushEvent(
          `Failed to create instance: ${err instanceof Error ? err.message : "Unknown error"}`,
          "error",
        );
      }
    },
    [createInstanceApi, pushEvent],
  );

  // Build inspector props for selected component
  const getInspectorProps = () => {
    if (!selected) return null;

    if (selected.type === "source") {
      const source = sources.find((s) => s.id === selected.id);
      if (!source) return null;
      const connectedQueries = queries
        .filter((q) => q.sources.some((s) => s.sourceId === selected.id))
        .map((q) => ({
          id: q.id,
          type: "query" as ComponentType,
          status: (q.status ?? "Stopped") as ComponentStatus,
        }));

      return {
        title: source.id,
        subtitle: `${source.kind} source`,
        componentType: "source" as ComponentType,
        status: source.status as ComponentStatus,
        error: source.error,
        config: { kind: source.kind, autoStart: source.autoStart },
        connections: connectedQueries,
        onStart: () => {
          startSource(source.id);
          pushEvent(`Started source: ${source.id}`, "success");
        },
        onStop: () => {
          stopSource(source.id);
          pushEvent(`Stopped source: ${source.id}`, "warning");
        },
        onDelete: () => {
          removeSource(source.id);
          pushEvent(`Deleted source: ${source.id}`, "error");
          setSelected(null);
        },
      };
    }

    if (selected.type === "query") {
      const query = queries.find((q) => q.id === selected.id);
      if (!query) return null;
      const connectedSources = query.sources.map((s) => {
        const src = sources.find((ss) => ss.id === s.sourceId);
        return {
          id: s.sourceId,
          type: "source" as ComponentType,
          status: (src?.status ?? "Stopped") as ComponentStatus,
        };
      });
      const connectedReactions = reactions
        .filter((r) => r.queries.includes(selected.id))
        .map((r) => ({
          id: r.id,
          type: "reaction" as ComponentType,
          status: r.status as ComponentStatus,
        }));

      return {
        title: query.id,
        subtitle: "continuous query",
        componentType: "query" as ComponentType,
        status: (query.status ?? "Stopped") as ComponentStatus,
        error: query.error,
        config: {
          query: query.query,
          language: query.queryLanguage ?? "Cypher",
          sources: query.sources.map((s) => s.sourceId).join(", "),
        },
        connections: [...connectedSources, ...connectedReactions],
        onStart: () => {
          startQuery(query.id);
          pushEvent(`Started query: ${query.id}`, "success");
        },
        onStop: () => {
          stopQuery(query.id);
          pushEvent(`Stopped query: ${query.id}`, "warning");
        },
        onDelete: () => {
          removeQuery(query.id);
          pushEvent(`Deleted query: ${query.id}`, "error");
          setSelected(null);
        },
      };
    }

    if (selected.type === "reaction") {
      const reaction = reactions.find((r) => r.id === selected.id);
      if (!reaction) return null;
      const connectedQs = reaction.queries.map((qId) => {
        const q = queries.find((qq) => qq.id === qId);
        return {
          id: qId,
          type: "query" as ComponentType,
          status: (q?.status ?? "Stopped") as ComponentStatus,
        };
      });

      return {
        title: reaction.id,
        subtitle: `${reaction.kind} reaction`,
        componentType: "reaction" as ComponentType,
        status: reaction.status as ComponentStatus,
        error: reaction.error,
        config: {
          kind: reaction.kind,
          queries: reaction.queries.join(", "),
          autoStart: reaction.autoStart,
        },
        connections: connectedQs,
        onStart: () => {
          startReaction(reaction.id);
          pushEvent(`Started reaction: ${reaction.id}`, "success");
        },
        onStop: () => {
          stopReaction(reaction.id);
          pushEvent(`Stopped reaction: ${reaction.id}`, "warning");
        },
        onDelete: () => {
          removeReaction(reaction.id);
          pushEvent(`Deleted reaction: ${reaction.id}`, "error");
          setSelected(null);
        },
      };
    }

    return null;
  };

  const inspectorProps = getInspectorProps();

  const isEmpty =
    sources.length === 0 && queries.length === 0 && reactions.length === 0;

  // Determine accent color for CreatePanel based on draft type
  const draftAccent =
    draft?.componentType === "source"
      ? "#3b82f6"
      : draft?.componentType === "query"
        ? "#8b5cf6"
        : "#06b6d4";

  const draftTitle = draft
    ? draft.componentType === "source"
      ? `New ${draft.kind} Source`
      : draft.componentType === "query"
        ? "New Query"
        : `New ${draft.kind} Reaction`
    : "";

  // If a URL-requested instance wasn't found, show the picker instead of the main UI
  if (requestedNotFound && !selectedInstanceId) {
    return (
      <>
        <InstancePickerDialog
          instances={instances}
          missingId={requestedNotFound}
          onSelect={setSelectedInstanceId}
          onCreateNew={() => {
            setCreateInstancePrefilledId(requestedNotFound);
            setShowCreateInstance(true);
          }}
        />
        {showCreateInstance && (
          <CreateInstanceDialog
            onSave={handleCreateInstance}
            onCancel={() => {
              setShowCreateInstance(false);
              setCreateInstancePrefilledId(undefined);
            }}
            initialId={createInstancePrefilledId}
          />
        )}
      </>
    );
  }

  return (
    <AppLayout
      onAddComponent={() => setCreateStep("component")}
      connected={connected}
      onToggleActivity={() => setActivityOpen((p) => !p)}
      eventCount={events.length}
      theme={theme}
      onToggleTheme={toggleTheme}
      instanceSlot={
        <InstanceSelector
          instances={instances}
          selectedId={selectedInstanceId}
          onSelect={setSelectedInstanceId}
          onCreateNew={() => setShowCreateInstance(true)}
        />
      }
    >
      {/* Flow Canvas */}
      {isEmpty ? (
        <div className="w-full h-full flex flex-col items-center justify-center gap-4 text-drasi-text-secondary">
          <p className="text-lg font-semibold text-drasi-text-primary">
            No components yet
          </p>
          <p className="text-sm max-w-md text-center">
            Click <strong>Add</strong> above to create your first Source, Query,
            or Reaction — or start the server with a config file.
          </p>
          <button
            onClick={() => setCreateStep("component")}
            className="action-btn-primary mt-2"
          >
            + Add
          </button>
        </div>
      ) : (
        <FlowCanvas data={pipelineData} instanceId={selectedInstanceId} onNodeClick={handleNodeClick} />
      )}

      {/* Inspector Panel */}
      {inspectorProps && (
        <InspectorPanel {...inspectorProps} onClose={() => setSelected(null)} />
      )}

      {/* Type Selector Overlay */}
      {createStep && (
        <TypeSelector
          level={createStep}
          onSelect={handleCreateSelect}
          onSelectSolution={(templateId) => {
            setCreateStep(null);
            setDeployTemplateId(templateId);
          }}
          onUploadSolution={(yaml) => {
            setCreateStep(null);
            setDeployUploadedYaml(yaml);
          }}
          onCancel={() => setCreateStep(null)}
        />
      )}

      {/* Create Panel — form for the selected kind */}
      {draft && (
        <CreatePanel
          draft={draft}
          title={draftTitle}
          subtitle={draft.componentType}
          accentColor={draftAccent}
          onSave={handleSaveDraft}
          onCancel={discard}
        >
          {draft.componentType === "source" && (
            <SourceForm
              kind={draft.kind}
              fields={draft.fields}
              errors={draft.errors}
              onChange={updateField}
            />
          )}
          {draft.componentType === "query" && (
            <QueryForm
              fields={draft.fields}
              errors={draft.errors}
              onChange={updateField}
              availableSources={sources.map((s) => ({
                id: s.id,
                kind: s.kind,
              }))}
            />
          )}
          {draft.componentType === "reaction" && (
            <ReactionForm
              kind={draft.kind}
              fields={draft.fields}
              errors={draft.errors}
              onChange={updateField}
              availableQueries={queries.map((q) => ({ id: q.id }))}
            />
          )}
        </CreatePanel>
      )}

      {/* Create Instance Dialog */}
      {showCreateInstance && (
        <CreateInstanceDialog
          onSave={handleCreateInstance}
          onCancel={() => {
            setShowCreateInstance(false);
            setCreateInstancePrefilledId(undefined);
          }}
          initialId={createInstancePrefilledId}
        />
      )}

      {/* Activity Panel */}
      <EventPanel
        events={events}
        open={activityOpen}
        onClose={() => setActivityOpen(false)}
        onClear={() => setEvents([])}
      />

      {/* Solution Deploy Dialog */}
      {(deployTemplateId || deployUploadedYaml) && (
        <SolutionDeployDialog
          templateId={deployTemplateId}
          uploadedYaml={deployUploadedYaml}
          onClose={() => {
            setDeployTemplateId(undefined);
            setDeployUploadedYaml(undefined);
          }}
          onSuccess={(deployedToInstanceId) => {
            setDeployTemplateId(undefined);
            setDeployUploadedYaml(undefined);
            // Refresh instances list (in case a new one was created)
            refreshInstances();
            // Switch to the instance that was deployed to
            setSelectedInstanceId(deployedToInstanceId);
            // Refresh components for that instance
            refreshSources();
            refreshQueries();
            refreshReactions();
          }}
        />
      )}
    </AppLayout>
  );
}
