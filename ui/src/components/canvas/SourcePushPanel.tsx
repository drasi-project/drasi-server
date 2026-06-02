import { useState, useCallback } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { Send, Check, AlertTriangle } from "lucide-react";

interface SourcePushPanelProps {
  sourceId: string;
  instanceId?: string;
  host: string;
  port: number;
  endpoint?: string;
}

type Operation = "insert" | "update" | "delete";
type SendStatus = "idle" | "sending" | "success" | "error";

const TEMPLATE: Record<Operation, string> = {
  insert: `{
  "type": "node",
  "id": "item-1",
  "labels": ["Item"],
  "properties": {
    "name": "Example"
  }
}`,
  update: `{
  "type": "node",
  "id": "item-1",
  "labels": ["Item"],
  "properties": {
    "name": "Updated"
  }
}`,
  delete: `{
  "type": "node",
  "id": "item-1",
  "labels": ["Item"],
  "properties": {}
}`,
};

const OP_STYLES: Record<Operation, string> = {
  insert: "bg-drasi-running/20 text-drasi-running border-drasi-running/40",
  update: "bg-drasi-warning/20 text-drasi-warning border-drasi-warning/40",
  delete: "bg-drasi-error/20 text-drasi-error border-drasi-error/40",
};

export default function SourcePushPanel({
  sourceId,
  instanceId,
  host,
  port,
  endpoint,
}: SourcePushPanelProps) {
  const [operation, setOperation] = useState<Operation>("insert");
  const [payload, setPayload] = useState(TEMPLATE.insert);
  const [status, setStatus] = useState<SendStatus>("idle");
  const [errorMsg, setErrorMsg] = useState("");

  // Proxy URL through drasi-server to avoid CORS
  const proxyUrl = instanceId
    ? `/api/v1/instances/${instanceId}/sources/${sourceId}/push`
    : `/api/v1/sources/${sourceId}/push`;

  const effectiveHost =
    host === "0.0.0.0" ? "localhost" : host;
  const basePath = endpoint
    ? `/${endpoint.replace(/^\//, "")}`
    : "";
  const directUrl = `http://${effectiveHost}:${port}${basePath}/sources/${sourceId}/events`;

  const handleOperationChange = (op: Operation) => {
    setOperation(op);
    setPayload(TEMPLATE[op]);
    setStatus("idle");
  };

  const handleSend = useCallback(async () => {
    setStatus("sending");
    setErrorMsg("");
    try {
      const element = JSON.parse(payload);
      const body = { operation, element };
      const res = await fetch(proxyUrl, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });
      if (res.ok) {
        setStatus("success");
        setTimeout(() => setStatus("idle"), 2000);
      } else {
        setStatus("error");
        setErrorMsg(`HTTP ${res.status}`);
      }
    } catch (err) {
      setStatus("error");
      setErrorMsg(
        err instanceof Error ? err.message : "Request failed",
      );
    }
  }, [operation, payload, proxyUrl]);

  return (
    <div className="mt-3 pt-3 border-t border-drasi-border space-y-2">
      {/* Operation selector — game action bar */}
      <div className="flex gap-1 nodrag">
        {(["insert", "update", "delete"] as Operation[]).map((op) => (
          <button
            key={op}
            onClick={() => handleOperationChange(op)}
            className={`flex-1 px-2 py-1 rounded text-[10px] font-bold uppercase tracking-wider
              border transition-all ${
                operation === op
                  ? OP_STYLES[op]
                  : "bg-drasi-bg text-drasi-text-secondary border-drasi-border hover:border-drasi-text-secondary"
              }`}
          >
            {op}
          </button>
        ))}
      </div>

      {/* JSON payload editor */}
      <textarea
        value={payload}
        onChange={(e) => {
          setPayload(e.target.value);
          setStatus("idle");
        }}
        className="nowheel nodrag w-full h-32 bg-drasi-bg border border-drasi-border rounded-lg p-2
                   font-mono text-[11px] text-drasi-text-primary resize-none
                   focus:outline-none focus:border-drasi-source/60 transition-colors"
        spellCheck={false}
      />

      {/* Send button + status */}
      <div className="flex items-center gap-2 nodrag">
        <motion.button
          onClick={handleSend}
          disabled={status === "sending"}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-semibold
                     bg-drasi-source text-white transition-all
                     hover:shadow-glow-source disabled:opacity-50"
          whileHover={{ scale: 1.05 }}
          whileTap={{ scale: 0.95 }}
        >
          <Send size={12} />
          {status === "sending" ? "Sending..." : "Send"}
        </motion.button>

        <AnimatePresence>
          {status === "success" && (
            <motion.span
              initial={{ opacity: 0, x: -8 }}
              animate={{ opacity: 1, x: 0 }}
              exit={{ opacity: 0 }}
              className="flex items-center gap-1 text-[10px] text-drasi-running"
            >
              <Check size={12} /> Sent!
            </motion.span>
          )}
          {status === "error" && (
            <motion.span
              initial={{ opacity: 0, x: -8 }}
              animate={{ opacity: 1, x: 0 }}
              exit={{ opacity: 0 }}
              className="flex items-center gap-1 text-[10px] text-drasi-error"
              title={errorMsg}
            >
              <AlertTriangle size={12} /> {errorMsg || "Failed"}
            </motion.span>
          )}
        </AnimatePresence>
      </div>

      {/* URL indicator */}
      <div
        className="text-[9px] font-mono text-drasi-text-secondary/50 truncate"
        title={directUrl}
      >
        → {directUrl}
      </div>
    </div>
  );
}
