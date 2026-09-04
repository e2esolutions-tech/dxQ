/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        // ree0xQ palette: posture-band greens / yellows / reds aligned
        // with the paper's q thresholds (plan/migration/must-migrate).
        posture: {
          good: "#16a34a",
          plan: "#eab308",
          urgent: "#ea580c",
          critical: "#b91c1c",
          blocked: "#7c2d12",
        },
        ink: {
          50: "#f8fafc",
          100: "#f1f5f9",
          200: "#e2e8f0",
          400: "#94a3b8",
          600: "#475569",
          800: "#1e293b",
          900: "#0f172a",
        },
      },
      fontFamily: {
        mono: ["ui-monospace", "SFMono-Regular", "Menlo", "monospace"],
      },
    },
  },
  plugins: [],
};
