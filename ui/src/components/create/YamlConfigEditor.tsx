import Editor, { loader } from "@monaco-editor/react";
import { configureMonacoYaml } from "monaco-yaml";
import type { MonacoYaml } from "monaco-yaml";
import { useRef, useEffect, useState } from "react";
import type { JSONSchema } from "monaco-yaml";
import yaml from "js-yaml";

let monacoYamlInstance: MonacoYaml | null = null;

interface YamlConfigEditorProps {
  schema: Record<string, unknown>;
  value: string;
  onChange: (yaml: string) => void;
}

export default function YamlConfigEditor({
  schema,
  value,
  onChange,
}: YamlConfigEditorProps) {
  const [isDark, setIsDark] = useState(
    document.documentElement.classList.contains("dark"),
  );
  const initRef = useRef(false);

  // Watch for theme changes
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

  // Configure monaco-yaml with schema
  useEffect(() => {
    if (initRef.current) {
      // Update existing instance
      if (monacoYamlInstance) {
        monacoYamlInstance.update({
          schemas: [
            {
              uri: "drasi://config-schema.json",
              fileMatch: ["*"],
              schema: schema as JSONSchema,
            },
          ],
        });
      }
      return;
    }
    initRef.current = true;

    loader.init().then((monaco) => {
      // Define themes for YAML editor
      monaco.editor.defineTheme("drasi-yaml-dark", {
        base: "vs-dark",
        inherit: true,
        rules: [],
        colors: {
          "editor.background": "#0a0e17",
          "editor.foreground": "#f1f5f9",
          "editorLineNumber.foreground": "#475569",
        },
      });
      monaco.editor.defineTheme("drasi-yaml-light", {
        base: "vs",
        inherit: true,
        rules: [],
        colors: {
          "editor.background": "#e2e8f0",
          "editor.foreground": "#0f172a",
          "editorLineNumber.foreground": "#94a3b8",
        },
      });

      monacoYamlInstance = configureMonacoYaml(monaco, {
        validate: true,
        schemas: [
          {
            uri: "drasi://config-schema.json",
            fileMatch: ["*"],
            schema: schema as JSONSchema,
          },
        ],
      });
    });
  }, [schema]);

  return (
    <div className="rounded-xl border border-drasi-border overflow-hidden">
      <Editor
        height="240px"
        language="yaml"
        value={value}
        theme={isDark ? "drasi-yaml-dark" : "drasi-yaml-light"}
        onChange={(v) => onChange(v ?? "")}
        options={{
          minimap: { enabled: false },
          scrollBeyondLastLine: false,
          lineNumbers: "on",
          fontSize: 12,
          fontFamily:
            "'JetBrains Mono', 'Fira Code', 'Cascadia Code', 'Menlo', monospace",
          tabSize: 2,
          automaticLayout: true,
          padding: { top: 8, bottom: 8 },
          renderLineHighlight: "none",
          overviewRulerLanes: 0,
          overviewRulerBorder: false,
          scrollbar: {
            vertical: "auto",
            horizontal: "auto",
            verticalScrollbarSize: 6,
            horizontalScrollbarSize: 6,
          },
        }}
      />
    </div>
  );
}

/**
 * Convert a form-data object into a pretty YAML string.
 * Strips keys that are empty strings/undefined/null so the YAML stays clean.
 */
export function formDataToYaml(data: Record<string, unknown>): string {
  const cleaned: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(data)) {
    if (v !== undefined && v !== null && v !== "") {
      cleaned[k] = v;
    }
  }
  return yaml.dump(cleaned, { indent: 2, noRefs: true, sortKeys: true });
}

/**
 * Parse a YAML string back into a form-data object.
 * Returns empty object on parse failure.
 */
export function yamlToFormData(text: string): Record<string, unknown> {
  try {
    const parsed = yaml.load(text);
    if (typeof parsed === "object" && parsed !== null && !Array.isArray(parsed)) {
      return parsed as Record<string, unknown>;
    }
    return {};
  } catch {
    return {};
  }
}
