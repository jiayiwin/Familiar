import { defineConfig } from "vite";
import { resolve } from "node:path";

export default defineConfig({
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    target: "es2021",
    outDir: "dist",
    emptyOutDir: true,
    rollupOptions: {
      input: {
        pet: resolve(__dirname, "index.html"),
        card: resolve(__dirname, "card.html"),
        gallery: resolve(__dirname, "gallery.html"),
        welcome: resolve(__dirname, "welcome.html"),
        overlay: resolve(__dirname, "overlay.html"),
        report: resolve(__dirname, "report.html"),
        menu: resolve(__dirname, "menu.html"),
      },
    },
  },
});
