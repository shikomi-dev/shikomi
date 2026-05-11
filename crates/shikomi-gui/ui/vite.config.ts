import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

// Tauri の推奨設定
// 出典: https://v2.tauri.app/start/frontend/solidjs/
export default defineConfig({
  plugins: [solid()],
  // Tauri CLI が TAURI_DEBUG / TAURI_ENV_TARGET_TRIPLE 等の環境変数を設定する
  build: {
    target: ["es2021", "chrome100", "safari13"],
    minify: !process.env.TAURI_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_DEBUG,
  },
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    watch: {
      // Tauri Rust ソースの変更は Vite HMR と無関係
      ignored: ["**/src/**", "**/target/**"],
    },
  },
});
