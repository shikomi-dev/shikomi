import { defineConfig } from "vitest/config";
import solid from "vite-plugin-solid";
import { resolve } from "path";

export default defineConfig({
  plugins: [solid()],
  resolve: {
    alias: {
      // テスト専用エイリアス: "@tests/..." → "./tests/..."
      "@tests": resolve(__dirname, "./tests"),
    },
  },
  test: {
    environment: "happy-dom",
    globals: false,
    setupFiles: ["./tests/setup.ts"],
    include: ["src/**/*.test.tsx", "src/**/*.test.ts", "src/**/*.it.test.tsx"],
  },
});
