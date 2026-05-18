/**
 * Monaco Editor local worker setup for Monaco 0.52.
 *
 * Monaco 0.52 uses the classic createWebWorker API (moduleId-based),
 * not the 0.55+ Worker-based API. Workers are classic scripts (not ES modules).
 */

import * as monaco from "monaco-editor";
import { loader } from "@monaco-editor/react";

import editorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";
import jsonWorker from "monaco-editor/esm/vs/language/json/json.worker?worker";
import yamlWorker from "monaco-yaml/yaml.worker?worker";

self.MonacoEnvironment = {
  getWorker(_workerId: string, label: string) {
    switch (label) {
      case "yaml":
        return new yamlWorker();
      case "json":
        return new jsonWorker();
      default:
        return new editorWorker();
    }
  },
};

loader.config({ monaco });
