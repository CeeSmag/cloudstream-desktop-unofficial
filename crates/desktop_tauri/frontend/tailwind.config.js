/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{js,ts,jsx,tsx}'],
  theme: {
    extend: {
      colors: {
        midnight: '#0C0C16',
        surface: '#161624',
        'surface-elevated': '#1E1E32',
        'surface-glass': 'rgba(22, 22, 36, 0.72)',
        'surface-border': 'rgba(255, 255, 255, 0.08)',
        accent: {
          DEFAULT: '#7C6BFF',
          hover: '#9385FF',
          glow: 'rgba(124, 107, 255, 0.35)',
        },
        gold: '#FFD15C',
      },
      fontFamily: {
        sans: ['Inter', 'system-ui', '-apple-system', 'sans-serif'],
      },
      backdropBlur: {
        xs: '2px',
        md: '12px',
        lg: '20px',
      },
    },
  },
  plugins: [],
};
