/** @type {import('tailwindcss').Config} */
export default {
  darkMode: 'class',
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        border: "hsl(var(--border))",
        input: "hsl(var(--input))",
        ring: "hsl(var(--ring))",
        background: "hsl(var(--background))",
        foreground: "hsl(var(--foreground))",
        primary: {
          DEFAULT: "hsl(var(--primary))",
          foreground: "hsl(var(--primary-foreground))",
        },
        secondary: {
          DEFAULT: "hsl(var(--secondary))",
          foreground: "hsl(var(--secondary-foreground))",
        },
        destructive: {
          DEFAULT: "hsl(var(--destructive))",
          foreground: "hsl(var(--destructive-foreground))",
        },
        muted: {
          DEFAULT: "hsl(var(--muted))",
          foreground: "hsl(var(--muted-foreground))",
        },
        accent: {
          DEFAULT: "hsl(var(--accent))",
          foreground: "hsl(var(--accent-foreground))",
        },
        card: {
          DEFAULT: "hsl(var(--card))",
          foreground: "hsl(var(--card-foreground))",
        },
        state: {
          running: '#4CAF50',
          'running-bg': '#E8F5E9',
          degraded: '#FF9800',
          'degraded-bg': '#FFF3E0',
          failed: '#F44336',
          'failed-bg': '#FFEBEE',
          stopped: '#9E9E9E',
          'stopped-bg': '#F5F5F5',
          starting: '#2196F3',
          'starting-bg': '#E3F2FD',
          stopping: '#2196F3',
          'stopping-bg': '#E3F2FD',
          unreachable: '#212121',
          unknown: '#BDBDBD',
        },
      },
      borderRadius: {
        lg: "var(--radius)",
        md: "calc(var(--radius) - 2px)",
        sm: "calc(var(--radius) - 4px)",
      },
      keyframes: {
        pulse: {
          '0%, 100%': { opacity: '1' },
          '50%': { opacity: '0.5' },
        },
      },
      animation: {
        'state-pulse': 'pulse 1.5s ease-in-out infinite',
      },
      width: {
        'sidebar-collapsed': '60px',
        'sidebar-expanded': '240px',
      },
    },
  },
  plugins: [],
}
