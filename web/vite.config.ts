import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      // Proxy collector API to sezar-server during development.
      "/v1": {
        target: "http://127.0.0.1:8090",
        changeOrigin: true,
      },
      "/healthz": {
        target: "http://127.0.0.1:8090",
        changeOrigin: true,
      },
    },
  },
});
