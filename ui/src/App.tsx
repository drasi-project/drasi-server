import { useState, useCallback, useEffect, useMemo } from "react";
import { AnimatePresence, motion } from "framer-motion";
import AppLayout from "@/layouts/AppLayout";
import FlowCanvas from "@/components/canvas/FlowCanvas";
import TypeSelector, {
  type SelectableType,
} from "@/components/create/TypeSelector";
import CreatePanel from "@/components/create/CreatePanel";
import SourceForm from "@/components/create/SourceForms";
import QueryForm from "@/components/create/QueryForm";
import ReactionForm from "@/components/create/ReactionForms";
import type { EventEntry } from "@/components/events/EventPanel";
import LeftPanel from "@/components/sidebar/LeftPanel";
import IconRail, { type SidebarTab } from "@/components/sidebar/IconRail";
import CurrentComponentPanel from "@/components/sidebar/CurrentComponentPanel";
import type { InspectorData } from "@/components/sidebar/CurrentComponentPanel";
import ComponentsPanel from "@/components/sidebar/ComponentsPanel";
import SolutionTemplatesPanel from "@/components/sidebar/SolutionTemplatesPanel";
import PluginsPanel from "@/components/sidebar/PluginsPanel";
import InstancesPanel from "@/components/sidebar/InstancesPanel";
import LogsPanel from "@/components/sidebar/LogsPanel";
import InstanceSelector from "@/components/instances/InstanceSelector";
import InstancePickerDialog from "@/components/instances/InstancePickerDialog";
import CreateInstanceDialog from "@/components/instances/CreateInstanceDialog";
import CloneInstanceDialog from "@/components/instances/CloneInstanceDialog";
import SolutionDeployDialog from "@/components/solutions/SolutionDeployDialog";
import SolutionInstanceWizard from "@/components/solutions/SolutionInstanceWizard";
import CreateSolutionTemplateDialog from "@/components/solutions/CreateSolutionTemplateDialog";
import { useSources, useQueries, useReactions } from "@/hooks/useApi";
import { useInstances } from "@/hooks/useInstances";
import { useConnectionState } from "@/hooks/useConnectionState";
import { useComponentEventLog } from "@/hooks/useComponentEventLog";
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
  const [showCloneInstance, setShowCloneInstance] = useState(false);
  const [showCreateTemplate, setShowCreateTemplate] = useState(false);
  const [showSolutionInstanceWizard, setShowSolutionInstanceWizard] = useState(false);
  const [createInstancePrefilledId, setCreateInstancePrefilledId] = useState<string | undefined>(undefined);

  // Component hooks - scoped to selected instance
  const {
    sources,
    create: createSourceApi,
    start: startSource,
    stop: stopSource,
    remove: removeSource,
  } = useSources(selectedInstanceId);
  const {
    queries,
    create: createQueryApi,
    start: startQuery,
    stop: stopQuery,
    remove: removeQuery,
  } = useQueries(selectedInstanceId);
  const {
    reactions,
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

  // Connection state — reactive via SSE, no polling
  const connectionState = useConnectionState(selectedInstanceId ?? undefined);

  // SSE ComponentGraph event log for the Logs panel
  const { entries: sseEvents, clear: clearSseEvents } = useComponentEventLog(selectedInstanceId ?? undefined);

  // Merge SSE events and user-action events (errors, success messages) into a
  // single sorted stream for the Logs panel. Both arrays are already in reverse
  // chronological order; we merge and re-sort to interleave correctly.
  const mergedEvents = useMemo(() => {
    return [...events, ...sseEvents].sort(
      (a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime(),
    );
  }, [events, sseEvents]);

  const clearAllEvents = useCallback(() => {
    setEvents([]);
    clearSseEvents();
  }, [clearSseEvents]);

  // Sidebar state
  const [sidebarTab, setSidebarTab] = useState<SidebarTab | null>(() => {
    try {
      return (localStorage.getItem("drasi-sidebar-tab") as SidebarTab) || null;
    } catch {
      return null;
    }
  });
  const [sidebarPinned, setSidebarPinned] = useState(() => {
    try {
      return localStorage.getItem("drasi-sidebar-pinned") === "true";
    } catch {
      return false;
    }
  });

  // Persist sidebar state
  useEffect(() => {
    try {
      if (sidebarTab) {
        localStorage.setItem("drasi-sidebar-tab", sidebarTab);
      } else {
        localStorage.removeItem("drasi-sidebar-tab");
      }
    } catch { /* ignore */ }
  }, [sidebarTab]);
  useEffect(() => {
    try {
      localStorage.setItem("drasi-sidebar-pinned", String(sidebarPinned));
    } catch { /* ignore */ }
  }, [sidebarPinned]);

  // Solution deploy state
  const [deployTemplateId, setDeployTemplateId] = useState<string | undefined>(undefined);
  const [deployUploadedYaml, setDeployUploadedYaml] = useState<string | undefined>(undefined);

  // Build pipeline data for the canvas (memoized to avoid cascade re-renders)
  const pipelineData: PipelineData = useMemo(() => ({
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
      instanceId: selectedInstanceId,
    })),
  }), [sources, queries, reactions, selectedInstanceId]);

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

  const handlePaneClick = useCallback(() => {
    setSelected(null);
    if (!sidebarPinned) {
      setSidebarTab(null);
    }
  }, [sidebarPinned]);

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
      // SSE "Added" events will update the canvas reactively
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

  // Build inspector props for selected component (memoized to avoid rebuilding every render)
  const inspectorProps = useMemo(() => {
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
        isSource: true as const,
        id: source.id,
        kind: source.kind,
        status: source.status as ComponentStatus,
        error: source.error,
        autoStart: source.autoStart,
        properties: source.properties,
        queries: connectedQueries,
        onStart: async () => {
          try {
            await startSource(source.id);
          } catch (err) {
            pushEvent(`Failed to start source '${source.id}': ${err instanceof Error ? err.message : "Unknown error"}`, "error");
          }
        },
        onStop: async () => {
          try {
            await stopSource(source.id);
          } catch (err) {
            pushEvent(`Failed to stop source '${source.id}': ${err instanceof Error ? err.message : "Unknown error"}`, "error");
          }
        },
        onDelete: async () => {
          try {
            await removeSource(source.id);
            setSelected(null);
          } catch (err) {
            pushEvent(`Failed to delete source '${source.id}': ${err instanceof Error ? err.message : "Unknown error"}`, "error");
          }
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
          kind: src?.kind,
        };
      });
      const connectedReactions = reactions
        .filter((r) => r.queries.includes(selected.id))
        .map((r) => ({
          id: r.id,
          type: "reaction" as ComponentType,
          status: r.status as ComponentStatus,
          kind: r.kind,
        }));

      return {
        isQuery: true as const,
        id: query.id,
        status: (query.status ?? "Stopped") as ComponentStatus,
        error: query.error,
        query: query.query,
        queryLanguage: query.queryLanguage ?? "Cypher",
        sources: connectedSources,
        reactions: connectedReactions,
        onStart: async () => {
          try {
            await startQuery(query.id);
          } catch (err) {
            pushEvent(`Failed to start query '${query.id}': ${err instanceof Error ? err.message : "Unknown error"}`, "error");
          }
        },
        onStop: async () => {
          try {
            await stopQuery(query.id);
          } catch (err) {
            pushEvent(`Failed to stop query '${query.id}': ${err instanceof Error ? err.message : "Unknown error"}`, "error");
          }
        },
        onDelete: async () => {
          try {
            await removeQuery(query.id);
            setSelected(null);
          } catch (err) {
            pushEvent(`Failed to delete query '${query.id}': ${err instanceof Error ? err.message : "Unknown error"}`, "error");
          }
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
        isReaction: true as const,
        id: reaction.id,
        kind: reaction.kind,
        status: reaction.status as ComponentStatus,
        error: reaction.error,
        autoStart: reaction.autoStart,
        properties: reaction.properties,
        queries: connectedQs,
        onStart: async () => {
          try {
            await startReaction(reaction.id);
          } catch (err) {
            pushEvent(`Failed to start reaction '${reaction.id}': ${err instanceof Error ? err.message : "Unknown error"}`, "error");
          }
        },
        onStop: async () => {
          try {
            await stopReaction(reaction.id);
          } catch (err) {
            pushEvent(`Failed to stop reaction '${reaction.id}': ${err instanceof Error ? err.message : "Unknown error"}`, "error");
          }
        },
        onDelete: async () => {
          try {
            await removeReaction(reaction.id);
            setSelected(null);
          } catch (err) {
            pushEvent(`Failed to delete reaction '${reaction.id}': ${err instanceof Error ? err.message : "Unknown error"}`, "error");
          }
        },
      };
    }

    return null;
  }, [selected, sources, queries, reactions, startSource, stopSource, removeSource, startQuery, stopQuery, removeQuery, startReaction, stopReaction, removeReaction, pushEvent]) as InspectorData | null;

  // Auto-switch to component tab when user selects a NEW component on the canvas
  useEffect(() => {
    if (selected) {
      setSidebarTab("component");
    }
  }, [selected?.id, selected?.type]);

  const isEmpty =
    sources.length === 0 && queries.length === 0 && reactions.length === 0;

  // Determine accent color for CreatePanel based on draft type
  const draftAccent =
    draft?.componentType === "source"
      ? "#22c55e"
      : draft?.componentType === "query"
        ? "#3b82f6"
        : "#8b5cf6";

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
      connectionState={connectionState}
      theme={theme}
      onToggleTheme={toggleTheme}
      instanceSlot={
        <InstanceSelector
          instances={instances}
          selectedId={selectedInstanceId}
          onSelect={setSelectedInstanceId}
        />
      }
    >
      <div className="flex h-full">
        {/* Icon rail — always visible, fixed width */}
        <div className="flex-shrink-0">
          <IconRail
            activeTab={sidebarTab}
            onTabClick={(tab) => {
              if (sidebarTab === tab) {
                if (!sidebarPinned) setSidebarTab(null);
              } else {
                setSidebarTab(tab);
              }
            }}
            badges={mergedEvents.length > 0 ? { logs: mergedEvents.length } : {}}
            componentSelected={selected !== null}
          />
        </div>

        {/* Sidebar content panel — slides in/out from left */}
        <AnimatePresence>
          {sidebarTab && (
            <motion.div
              key="sidebar-content"
              initial={{ width: 0 }}
              animate={{ width: 320 }}
              exit={{ width: 0 }}
              transition={{ duration: 0.2, ease: "easeInOut" }}
              className="flex-shrink-0 border-r border-[var(--drasi-border)] overflow-hidden"
            >
              <div style={{ width: 320 }} className="h-full">
                <LeftPanel
                  activeTab={sidebarTab}
                  pinned={sidebarPinned}
                  onTogglePin={() => setSidebarPinned((p) => !p)}
                >
                {sidebarTab === "component" && (
              <CurrentComponentPanel
                data={inspectorProps}
                onNavigate={(id, type) => setSelected({ id, type })}
                onStartSource={async (id) => {
                  try {
                    await startSource(id);
                  } catch (err) {
                    pushEvent(`Failed to start source '${id}': ${err instanceof Error ? err.message : "Unknown error"}`, "error");
                  }
                }}
                onStopSource={async (id) => {
                  try {
                    await stopSource(id);
                  } catch (err) {
                    pushEvent(`Failed to stop source '${id}': ${err instanceof Error ? err.message : "Unknown error"}`, "error");
                  }
                }}
                onStartQuery={async (id) => {
                  try {
                    await startQuery(id);
                  } catch (err) {
                    pushEvent(`Failed to start query '${id}': ${err instanceof Error ? err.message : "Unknown error"}`, "error");
                  }
                }}
                onStopQuery={async (id) => {
                  try {
                    await stopQuery(id);
                  } catch (err) {
                    pushEvent(`Failed to stop query '${id}': ${err instanceof Error ? err.message : "Unknown error"}`, "error");
                  }
                }}
                onStartReaction={async (id) => {
                  try {
                    await startReaction(id);
                  } catch (err) {
                    pushEvent(`Failed to start reaction '${id}': ${err instanceof Error ? err.message : "Unknown error"}`, "error");
                  }
                }}
                onStopReaction={async (id) => {
                  try {
                    await stopReaction(id);
                  } catch (err) {
                    pushEvent(`Failed to stop reaction '${id}': ${err instanceof Error ? err.message : "Unknown error"}`, "error");
                  }
                }}
              />
            )}
            {sidebarTab === "catalog" && (
              <ComponentsPanel
                onStartCreate={(componentType, kind) => {
                  if (componentType === "query") {
                    startDraft("query", "query");
                  } else if (kind) {
                    startDraft(componentType, kind);
                  } else {
                    // No kind specified — open the kind selector
                    setCreateStep(
                      componentType === "source" ? "source-kind" : "reaction-kind",
                    );
                  }
                }}
              />
            )}
            {sidebarTab === "solutions" && (
              <SolutionTemplatesPanel
                instanceId={selectedInstanceId ?? ""}
                sources={sources}
                queries={queries}
                reactions={reactions}
                onDeployTemplate={(templateId) => setDeployTemplateId(templateId)}
                onUploadYaml={(yaml) => setDeployUploadedYaml(yaml)}
                onCreateTemplate={() => setShowCreateTemplate(true)}
              />
            )}
            {sidebarTab === "plugins" && <PluginsPanel />}
            {sidebarTab === "instances" && (
              <InstancesPanel
                instances={instances}
                selectedId={selectedInstanceId}
                onSelect={setSelectedInstanceId}
                onCreateNew={() => setShowCreateInstance(true)}
                onCreateFromTemplate={() => setShowSolutionInstanceWizard(true)}
                onClone={() => setShowCloneInstance(true)}
                onCreateTemplate={() => setShowCreateTemplate(true)}
              />
            )}
            {sidebarTab === "logs" && (
              <LogsPanel events={mergedEvents} onClear={clearAllEvents} />
            )}
                </LeftPanel>
              </div>
            </motion.div>
          )}
        </AnimatePresence>

        {/* Main canvas area — fills remaining space */}
        <div className="flex-1 min-w-0">
          {isEmpty ? (
            <div className="w-full h-full flex flex-col items-center justify-center gap-4 text-drasi-text-secondary">
              <p className="text-lg font-semibold text-drasi-text-primary">
                No components yet
              </p>
              <p className="text-sm max-w-md text-center">
                Open the <strong>Components</strong> tab in the sidebar to
                create your first Source, Query, or Reaction.
              </p>
            </div>
          ) : (
            <FlowCanvas
              data={pipelineData}
              instanceId={selectedInstanceId}
              onNodeClick={handleNodeClick}
              onPaneClick={handlePaneClick}
            />
          )}
        </div>
      </div>

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

      {/* Clone Instance Dialog */}
      {showCloneInstance && selectedInstanceId && (
        <CloneInstanceDialog
          sourceInstanceId={selectedInstanceId}
          sourceComponentCounts={{
            sources: sources.length,
            queries: queries.length,
            reactions: reactions.length,
          }}
          onSuccess={(newInstanceId) => {
            setShowCloneInstance(false);
            refreshInstances();
            setSelectedInstanceId(newInstanceId);
          }}
          onCancel={() => setShowCloneInstance(false)}
        />
      )}

      {/* Create Solution Template Dialog */}
      {showCreateTemplate && selectedInstanceId && (
        <CreateSolutionTemplateDialog
          instanceId={selectedInstanceId}
          sources={sources}
          queries={queries}
          reactions={reactions}
          onSuccess={(templateId) => {
            setShowCreateTemplate(false);
            pushEvent(`Created solution template: ${templateId}`, "success");
          }}
          onCancel={() => setShowCreateTemplate(false)}
        />
      )}

      {/* Solution Instance Wizard */}
      {showSolutionInstanceWizard && (
        <SolutionInstanceWizard
          onClose={() => setShowSolutionInstanceWizard(false)}
          onSuccess={(newInstanceId) => {
            setShowSolutionInstanceWizard(false);
            pushEvent(
              `Created instance from template: ${newInstanceId}`,
              "success",
            );
            refreshInstances();
            setSelectedInstanceId(newInstanceId);
          }}
        />
      )}

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
            refreshInstances();
            setSelectedInstanceId(deployedToInstanceId);
          }}
        />
      )}
    </AppLayout>
  );
}
