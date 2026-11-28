// Website demo build: the real interface running against src/demo/tauri-mock.ts.
// Output goes to ../../docs/embed; see scripts/build-demo.sh.
import path from "node:path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const mock = path.resolve(import.meta.dirname, "src/demo/tauri-mock.ts");

export default defineConfig({
  plugins: [react(), tailwindcss()],
  base: "./",
  resolve: { alias: { "@tauri-apps/api/core": mock, "@tauri-apps/api/event": mock } },
  build: { target: "es2022", sourcemap: false, outDir: "../../docs/embed", emptyOutDir: true },
});
