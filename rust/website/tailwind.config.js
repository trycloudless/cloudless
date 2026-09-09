/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ["./src/**/*.rs", "../shared_ui/src/**/*.rs"],
  theme: {
    extend: {
      colors: {
        bg: "var(--color-bg)",
        surface: "var(--color-surface)",
        "surface-elevated": "var(--color-surface-elevated)",
        "text-primary": "var(--color-text-primary)",
        "text-secondary": "var(--color-text-secondary)",
        border: "var(--color-border)",
        input: "var(--color-input)",
        sidebar: "var(--color-sidebar)",
        primary: "var(--color-primary)",
        "primary-hover": "var(--color-primary-hover)",
        "primary-light": "var(--color-primary-light)",
        focus: "var(--color-focus)",
        accent: "var(--color-accent)",
        "accent-light": "var(--color-accent-light)",
        error: "var(--color-error)",
        warning: "var(--color-warning)",
        success: "var(--color-success)",
        info: "var(--color-info)",
        "surface-alpha": "var(--color-surface-alpha)",
        "border-alpha": "var(--color-border-alpha)",
        "input-alpha": "var(--color-input-alpha)",
        "primary-ring": "var(--color-primary-ring)",
        "primary-glow": "var(--color-primary-glow)",
        "primary-tint": "var(--color-primary-tint)",
        "accent-tint": "var(--color-accent-tint)",
        "error-tint": "var(--color-error-tint)",
        "error-border": "var(--color-error-border)",
        "warning-tint": "var(--color-warning-tint)",
        "success-tint": "var(--color-success-tint)",
      },
      borderRadius: {
        DEFAULT: "var(--radius-base)",
        lg: "var(--radius-lg)",
        sm: "var(--radius-sm)",
      },
      boxShadow: {
        elevated: "var(--shadow-elevated)",
      },
      fontFamily: {
        sans: ["Inter", "system-ui", "sans-serif"],
        display: ["Outfit", "system-ui", "sans-serif"],
      },
      animation: {
        "fade-in": "fadeIn 0.6s ease-out",
        "slide-up": "slideUp 0.4s ease-out",
      },
      keyframes: {
        fadeIn: {
          "0%": { opacity: "0", transform: "translateY(10px)" },
          "100%": { opacity: "1", transform: "translateY(0)" },
        },
        slideUp: {
          "0%": { opacity: "0", transform: "translateY(20px)" },
          "100%": { opacity: "1", transform: "translateY(0)" },
        },
      },
    },
  },
  plugins: [],
};
