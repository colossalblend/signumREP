import fs from "node:fs";
import path from "node:path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [
    react(),
    {
      name: "setup-index-html",
      closeBundle() {
        const from = path.resolve(__dirname, "setup-dist/setup.html");
        const to = path.resolve(__dirname, "setup-dist/index.html");
        if (fs.existsSync(from)) fs.renameSync(from, to);
      },
    },
  ],
  clearScreen: false,
  build: {
    outDir: "setup-dist",
    emptyOutDir: true,
    rollupOptions: {
      input: path.resolve(__dirname, "setup.html"),
    },
  },
});
