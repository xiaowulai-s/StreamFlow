/** @type {import('tailwindcss').Config} */
module.exports = {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        primary: {
          50: '#eff6ff',
          100: '#dbe8ff',
          200: '#c2d6ff',
          300: '#99bcff',
          400: '#66a3ff',
          500: '#2878F5',
          600: '#1d6ae8',
          700: '#1255d4',
          800: '#0f49b7',
          900: '#0f49b7',
        },
        cyan: {
          500: '#25BDEB',
        }
      },
    },
  },
  plugins: [],
}
