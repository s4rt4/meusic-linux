import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// WebKitGTK (Linux production) treats the `tauri://` custom scheme as an opaque
// origin, so the `crossorigin` attribute Vite puts on the built <script>/<link>
// triggers a CORS check that fails — the bundle never loads and the window is a
// black screen. (Dev is fine: it loads over normal `http://localhost`.) Strip
// the attribute from the emitted HTML so these same-origin asset loads aren't
// gated.
const stripCrossorigin = {
  name: "strip-crossorigin",
  transformIndexHtml(html: string) {
    return html.replace(/\s+crossorigin(?==|\s|>)/g, "");
  },
};

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react(), tailwindcss(), stripCrossorigin],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
