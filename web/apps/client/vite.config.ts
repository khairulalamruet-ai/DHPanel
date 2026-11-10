import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "path";

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5174,
    proxy: {
      "/api": {
        target: "http://localhost:3000",
        changeOrigin: true,
        secure: false,
      },
    },
  },
  resolve: {
    alias: {
      "@dhpanel/ui": path.resolve(__dirname, "../../packages/ui/src"),
      "@dhpanel/api": path.resolve(__dirname, "../../packages/api/src"),
      "@dhpanel/auth": path.resolve(__dirname, "../../packages/auth/src"),
      "@dhpanel/i18n": path.resolve(__dirname, "../../packages/i18n/src"),
      "@dhpanel/shell": path.resolve(__dirname, "../../packages/shell/src"),
      "@dhpanel/modules": path.resolve(__dirname, "../../packages/modules/src"),
    },
  },
});
