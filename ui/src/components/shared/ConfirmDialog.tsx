import { AlertTriangle } from "lucide-react";
import { motion, AnimatePresence } from "framer-motion";

interface ConfirmDialogProps {
  open: boolean;
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  variant?: "danger" | "warning";
  onConfirm: () => void;
  onCancel: () => void;
}

export default function ConfirmDialog({
  open,
  title,
  message,
  confirmLabel = "Confirm",
  cancelLabel = "Cancel",
  variant = "danger",
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const confirmClass =
    variant === "danger"
      ? "bg-drasi-error text-white hover:bg-red-600"
      : "bg-drasi-warning text-white hover:bg-amber-600";

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          className="fixed inset-0 z-[100] flex items-center justify-center bg-black/50"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.15 }}
          onClick={onCancel}
        >
          <motion.div
            className="bg-drasi-card border border-drasi-border rounded-xl p-5 max-w-sm space-y-4 shadow-xl"
            initial={{ scale: 0.95, opacity: 0 }}
            animate={{ scale: 1, opacity: 1 }}
            exit={{ scale: 0.95, opacity: 0 }}
            transition={{ duration: 0.15 }}
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-start gap-3">
              <div
                className={`p-2 rounded-lg ${
                  variant === "danger" ? "bg-drasi-error/20" : "bg-drasi-warning/20"
                }`}
              >
                <AlertTriangle
                  size={20}
                  className={variant === "danger" ? "text-drasi-error" : "text-drasi-warning"}
                />
              </div>
              <div>
                <h3 className="text-sm font-semibold text-drasi-text-primary">
                  {title}
                </h3>
                <p className="text-xs text-drasi-text-secondary mt-1">
                  {message}
                </p>
              </div>
            </div>
            <div className="flex gap-2 justify-end">
              <button
                onClick={onCancel}
                className="px-3 py-1.5 rounded-lg text-xs font-medium text-drasi-text-secondary hover:bg-drasi-surface transition-colors"
              >
                {cancelLabel}
              </button>
              <button
                onClick={onConfirm}
                className={`px-3 py-1.5 rounded-lg text-xs font-medium transition-colors ${confirmClass}`}
              >
                {confirmLabel}
              </button>
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
