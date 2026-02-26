/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        drasi: {
          bg: "var(--drasi-bg)",
          surface: "var(--drasi-surface)",
          card: "var(--drasi-card)",
          border: "var(--drasi-border)",
          "text-primary": "var(--drasi-text-primary)",
          "text-secondary": "var(--drasi-text-secondary)",
          source: "#22c55e",
          query: "#3b82f6",
          reaction: "#8b5cf6",
          running: "#10b981",
          warning: "#f59e0b",
          error: "#ef4444",
          stopped: "#64748b",
        },
      },
      boxShadow: {
        "glow-source": "0 0 12px 2px rgba(34, 197, 94, 0.4)",
        "glow-query": "0 0 12px 2px rgba(59, 130, 246, 0.4)",
        "glow-reaction": "0 0 12px 2px rgba(139, 92, 246, 0.4)",
        "glow-running": "0 0 12px 2px rgba(16, 185, 129, 0.4)",
        "glow-warning": "0 0 12px 2px rgba(245, 158, 11, 0.4)",
        "glow-error": "0 0 12px 2px rgba(239, 68, 68, 0.4)",
      },
      animation: {
        "pulse-glow": "pulseGlow 2s ease-in-out infinite",
        "flow-dot": "flowDot 2s linear infinite",
        "fade-in": "fadeIn 0.2s ease-out",
        "slide-in-right": "slideInRight 0.25s ease-out",
      },
      keyframes: {
        pulseGlow: {
          "0%, 100%": { opacity: "1" },
          "50%": { opacity: "0.6" },
        },
        flowDot: {
          "0%": { offsetDistance: "0%" },
          "100%": { offsetDistance: "100%" },
        },
        fadeIn: {
          "0%": { opacity: "0" },
          "100%": { opacity: "1" },
        },
        slideInRight: {
          "0%": { transform: "translateX(100%)", opacity: "0" },
          "100%": { transform: "translateX(0)", opacity: "1" },
        },
      },
    },
  },
  plugins: [],
};
