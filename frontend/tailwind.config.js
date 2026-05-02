/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        bg: "#0a0a0a",
        chrome: "#0e0e0e",
        panel: "#111111",
        panel2: "#161616",
        panel3: "#1c1c1c",
        border: "#222222",
        borderS: "#1a1a1a",
        borderL: "#2a2a2a",
        text: "#ededed",
        textDim: "#9a9a9a",
        textMute: "#6a6a6a",
        accent: "#10b981",
        accentDim: "#059669",
        amber: "#f59e0b",
        red: "#ef4444",
        blue: "#3b82f6",
        purple: "#a855f7",
        ok: "#10b981",
        warn: "#f59e0b",
        err: "#ef4444",
      },
      fontFamily: {
        sans: ["Inter", "-apple-system", "BlinkMacSystemFont", "Segoe UI", "system-ui", "sans-serif"],
        mono: ["JetBrains Mono", "ui-monospace", "SF Mono", "Menlo", "Consolas", "monospace"],
      },
    },
  },
  plugins: [],
};
