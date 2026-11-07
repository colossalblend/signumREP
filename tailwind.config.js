                                           
export default {
  darkMode: "class",
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      colors: {
        space: {
          950: "#070912",
          900: "#0b1020",
          850: "#10162a",
          800: "#151d34",
        },
        // Индиго вместо сиреневого: тот же холодный ряд, но темнее и плотнее.
        // Прежний #7c5cff — самый растиражированный оттенок в сгенерированных
        // интерфейсах, и на новых непрозрачных поверхностях он к тому же плохо
        // читался. Этот сохраняет узнаваемость, но выглядит как выбор, а не
        // как значение по умолчанию.
        brand: {
          300: "#a5b4fc",
          400: "#818cf8",
          500: "#5b6cf0",
          600: "#4453d8",
        },
      },
      backgroundImage: {
        // Было две фиолетовые радиальные подсветки по углам. Именно они и
        // читались как «ИИ-дизайн»: свет ниоткуда, без источника и смысла.
        // Оставляем едва заметный вертикальный переход — он даёт глубину,
        // но не притворяется освещением.
        "space-radial":
          "linear-gradient(180deg,rgba(255,255,255,.028) 0%,transparent 42%)",
        "brand-gradient": "linear-gradient(180deg,#5f70f2 0%,#4b5ce2 100%)",
        "blue-gradient": "linear-gradient(135deg,#1686ff 0%,#1556d7 100%)",
        "amber-gradient": "linear-gradient(135deg,#f5b72f 0%,#df7d14 100%)",
        dots:
          "radial-gradient(circle,rgba(255,255,255,.44) 1.15px,transparent 1.3px)",
      },
      boxShadow: {
        panel: "0 10px 30px rgba(0,0,0,.24), inset 0 1px rgba(255,255,255,.04)",
        icon: "0 18px 24px rgba(0,0,0,.38),0 7px 10px rgba(0,0,0,.28)",
        // Свечение убрано: настоящие приложения не светятся.
        glow: "0 8px 24px rgba(0,0,0,.30)",
      },
      dropShadow: {
        "icon-lg": ["0 18px 18px rgba(0,0,0,.45)", "0 6px 6px rgba(0,0,0,.32)", "0 0 18px rgba(255,255,255,.08)"],
      },
      fontFamily: {
        sans: ["Inter", "Segoe UI", "system-ui", "sans-serif"],
      },
    },
  },
  plugins: [],
};
