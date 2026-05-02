/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        bg: "var(--bg)",
        chrome: "var(--chrome)",
        panel: "var(--panel)",
        panel2: "var(--panel2)",
        panel3: "var(--panel3)",
        border: "var(--border)",
        borderS: "var(--borderS)",
        borderL: "var(--borderL)",
        text: "var(--text)",
        textDim: "var(--textDim)",
        textMute: "var(--textMute)",
        accent: "var(--accent)",
        accentDim: "var(--accentDim)",
        amber: "var(--amber)",
        red: "var(--red)",
        blue: "var(--blue)",
        purple: "var(--purple)",
        ok: "var(--accent)",
        warn: "var(--amber)",
        err: "var(--red)",
      },
      fontFamily: {
        sans: ["Inter", "-apple-system", "BlinkMacSystemFont", "Segoe UI", "system-ui", "sans-serif"],
        mono: ["JetBrains Mono", "ui-monospace", "SF Mono", "Menlo", "Consolas", "monospace"],
      },
    },
  },
  plugins: [],
};
