/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      colors: {
        hud: {
          bg: "rgba(15, 23, 42, 0.85)",
          border: "rgba(56, 189, 248, 0.4)",
          accent: "#38bdf8",
          warn: "#fbbf24",
          danger: "#f87171",
          success: "#4ade80",
        },
      },
      fontFamily: {
        mono: ["JetBrains Mono", "Fira Code", "monospace"],
      },
    },
  },
  plugins: [],
};
