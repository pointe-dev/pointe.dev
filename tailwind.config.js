/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./crates/frontend/src/**/*.rs",
    "./crates/frontend/src/**/*.html",
  ],
  theme: {
    extend: {
      colors: {
        black: "#0B0B0B",
        "pointe-red": "#D32F2F",
        "pointe-dark-red": "#E50914",
      },
      fontFamily: {
        serif: ["Playfair Display", "serif"],
        sans: ["Outfit", "Inter", "sans-serif"],
      },
      fontSize: {
        "display-lg": ["72px", { lineHeight: "1.2" }],
        "display-md": ["56px", { lineHeight: "1.2" }],
      },
    },
  },
  plugins: [],
};
