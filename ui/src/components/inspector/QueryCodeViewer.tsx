import { useRef, useEffect, useState, useMemo } from "react";
import Editor, { type OnMount, loader } from "@monaco-editor/react";
import type { editor } from "monaco-editor";
import { Maximize2, X } from "lucide-react";
import {
  cypherLanguageId,
  gqlLanguageId,
  cypherTokensProvider,
  gqlTokensProvider,
  cypherLanguageConfig,
} from "./cypher-language";

let languagesRegistered = false;

function registerLanguages() {
  if (languagesRegistered) return;
  languagesRegistered = true;

  loader.init().then((monaco) => {
    monaco.languages.register({ id: cypherLanguageId });
    monaco.languages.setMonarchTokensProvider(
      cypherLanguageId,
      cypherTokensProvider
    );
    monaco.languages.setLanguageConfiguration(
      cypherLanguageId,
      cypherLanguageConfig
    );

    monaco.languages.register({ id: gqlLanguageId });
    monaco.languages.setMonarchTokensProvider(gqlLanguageId, gqlTokensProvider);
    monaco.languages.setLanguageConfiguration(
      gqlLanguageId,
      cypherLanguageConfig
    );

    // Custom theme for light mode that works with Drasi's palette
    monaco.editor.defineTheme("drasi-light", {
      base: "vs",
      inherit: true,
      rules: [
        { token: "keyword", foreground: "7c3aed", fontStyle: "bold" },
        { token: "type.identifier", foreground: "0891b2" },
        { token: "support.function", foreground: "2563eb" },
        { token: "string", foreground: "16a34a" },
        { token: "string.escape", foreground: "15803d" },
        { token: "number", foreground: "d97706" },
        { token: "number.float", foreground: "d97706" },
        { token: "comment", foreground: "6b7280", fontStyle: "italic" },
        { token: "variable", foreground: "dc2626" },
        { token: "operator", foreground: "64748b" },
        { token: "delimiter.bracket", foreground: "64748b" },
        { token: "delimiter", foreground: "64748b" },
        { token: "identifier", foreground: "0f172a" },
      ],
      colors: {
        "editor.background": "#e2e8f0",
        "editor.foreground": "#0f172a",
        "editor.lineHighlightBackground": "#cbd5e100",
        "editorLineNumber.foreground": "#94a3b8",
      },
    });

    // Custom theme for dark mode
    monaco.editor.defineTheme("drasi-dark", {
      base: "vs-dark",
      inherit: true,
      rules: [
        { token: "keyword", foreground: "a78bfa", fontStyle: "bold" },
        { token: "type.identifier", foreground: "22d3ee" },
        { token: "support.function", foreground: "60a5fa" },
        { token: "string", foreground: "4ade80" },
        { token: "string.escape", foreground: "86efac" },
        { token: "number", foreground: "fbbf24" },
        { token: "number.float", foreground: "fbbf24" },
        { token: "comment", foreground: "6b7280", fontStyle: "italic" },
        { token: "variable", foreground: "f87171" },
        { token: "operator", foreground: "94a3b8" },
        { token: "delimiter.bracket", foreground: "94a3b8" },
        { token: "delimiter", foreground: "94a3b8" },
        { token: "identifier", foreground: "f1f5f9" },
      ],
      colors: {
        "editor.background": "#0a0e17",
        "editor.foreground": "#f1f5f9",
        "editor.lineHighlightBackground": "#1e293b00",
        "editorLineNumber.foreground": "#475569",
      },
    });
  });
}

/**
 * Format a query string by stripping blank leading/trailing lines
 * and removing common leading whitespace so the text is left-justified.
 */
function formatQuery(query: string): string {
  const lines = query.split("\n");

  while (lines.length > 0 && lines[0].trim() === "") lines.shift();
  while (lines.length > 0 && lines[lines.length - 1].trim() === "")
    lines.pop();

  if (lines.length === 0) return "";

  const nonEmptyLines = lines.filter((l) => l.trim() !== "");
  const minIndent = nonEmptyLines.reduce((min, line) => {
    const match = line.match(/^(\s*)/);
    return Math.min(min, match ? match[1].length : 0);
  }, Infinity);

  return lines
    .map((line) => (line.trim() === "" ? "" : line.slice(minIndent)))
    .join("\n");
}

const EDITOR_OPTIONS: editor.IStandaloneEditorConstructionOptions = {
  readOnly: true,
  domReadOnly: true,
  minimap: { enabled: false },
  scrollBeyondLastLine: false,
  lineNumbers: "off",
  glyphMargin: false,
  folding: false,
  lineDecorationsWidth: 0,
  lineNumbersMinChars: 0,
  renderLineHighlight: "none",
  overviewRulerLanes: 0,
  overviewRulerBorder: false,
  hideCursorInOverviewRuler: true,
  scrollbar: {
    vertical: "auto",
    horizontal: "auto",
    verticalScrollbarSize: 6,
    horizontalScrollbarSize: 6,
  },
  wordWrap: "off",
  contextmenu: false,
  fontSize: 13,
  fontFamily:
    "'JetBrains Mono', 'Fira Code', 'Cascadia Code', 'Menlo', monospace",
  padding: { top: 12, bottom: 12 },
  matchBrackets: "never",
  selectionHighlight: false,
  occurrencesHighlight: "off",
  renderValidationDecorations: "off",
  cursorStyle: "line-thin",
  cursorBlinking: "solid",
  guides: {
    indentation: false,
    bracketPairs: false,
  },
};

interface QueryCodeViewerProps {
  query: string;
  queryLanguage: string;
}

export default function QueryCodeViewer({
  query,
  queryLanguage,
}: QueryCodeViewerProps) {
  const editorRef = useRef<editor.IStandaloneCodeEditor | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [isDark, setIsDark] = useState(
    document.documentElement.classList.contains("dark")
  );

  // Watch for theme changes on <html>
  useEffect(() => {
    const observer = new MutationObserver(() => {
      setIsDark(document.documentElement.classList.contains("dark"));
    });
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class"],
    });
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (!expanded) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setExpanded(false);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [expanded]);

  // Register custom languages + themes once
  useEffect(() => {
    registerLanguages();
  }, []);

  const formattedQuery = useMemo(() => formatQuery(query), [query]);

  const monacoLanguage =
    queryLanguage.toLowerCase() === "gql" ? gqlLanguageId : cypherLanguageId;

  const MAX_HEIGHT = 500;

  const updateHeight = (ed: editor.IStandaloneCodeEditor) => {
    const contentHeight = ed.getContentHeight();
    const height = Math.min(contentHeight, MAX_HEIGHT);
    if (containerRef.current) {
      containerRef.current.style.height = `${height}px`;
    }
    ed.layout();
  };

  const handleMount: OnMount = (ed) => {
    editorRef.current = ed;
    updateHeight(ed);
    ed.onDidContentSizeChange(() => updateHeight(ed));
  };

  const theme = isDark ? "drasi-dark" : "drasi-light";

  return (
    <div className="relative">
      <button
        type="button"
        onClick={() => setExpanded(true)}
        className="absolute top-2 right-2 z-10 p-1 rounded-md bg-drasi-surface/80 border border-drasi-border text-drasi-text-secondary hover:text-drasi-text-primary"
        title="Expand query"
        aria-label="Expand query"
      >
        <Maximize2 className="w-3.5 h-3.5" />
      </button>
    <div
      ref={containerRef}
      className="bg-drasi-bg rounded-xl border border-drasi-border overflow-hidden"
    >
      <Editor
        height="100%"
        language={monacoLanguage}
        value={formattedQuery}
        theme={theme}
        onMount={handleMount}
        options={EDITOR_OPTIONS}
      />
    </div>
      {expanded && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm animate-fade-in"
          onClick={() => setExpanded(false)}
        >
          <div
            role="dialog"
            aria-modal="true"
            aria-label="Query"
            className="bg-drasi-surface border border-drasi-border rounded-2xl shadow-2xl w-[90vw] h-[85vh] flex flex-col"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="flex items-center justify-between px-4 py-3 border-b border-drasi-border">
              <h2 className="text-sm font-semibold text-drasi-text-primary">Query</h2>
              <button
                type="button"
                autoFocus
                onClick={() => setExpanded(false)}
                className="p-1 rounded-md text-drasi-text-secondary hover:text-drasi-text-primary"
                title="Close query"
                aria-label="Close query"
              >
                <X className="w-4 h-4" />
              </button>
            </div>
            <div className="flex-1 min-h-0 bg-drasi-bg rounded-b-2xl overflow-hidden">
              <Editor
                height="100%"
                language={monacoLanguage}
                value={formattedQuery}
                theme={theme}
                options={{ ...EDITOR_OPTIONS, fontSize: 14, wordWrap: "on" }}
              />
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
