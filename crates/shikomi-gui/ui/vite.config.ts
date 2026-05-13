import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

// Tauri の推奨設定
// 出典: https://v2.tauri.app/start/frontend/solidjs/
export default defineConfig({
  plugins: [solid()],
  // Tauri CLI が TAURI_DEBUG / TAURI_ENV_TARGET_TRIPLE 等の環境変数を設定する
  build: {
    // safari ターゲットは esbuild が destructuring を変換できないため除外。
    // esbuild "Transforming destructuring to the configured target environment is not supported yet"
    // Tauri v2 の macOS WebKit は常に最新を使用するため safari 指定不要。
    target: ["es2021", "chrome100"],
    minify: !process.env.TAURI_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_DEBUG,
  },
  clearScreen: false,
  // Vite 6 dev サーバの esbuild prebundle は build.target を参照せず、
  // 既定で古い browserslist 互換ターゲットを使うため、solid-js 内の
  // destructuring が "not supported yet" で失敗する。
  // 依存プリバンドル側のみ esnext に引き上げる（本番ビルド target は build.target を維持）。
  optimizeDeps: {
    esbuildOptions: {
      target: "esnext",
    },
  },
  server: {
    port: 5173,
    strictPort: true,
    watch: {
      // Tauri Rust ソースの変更は Vite HMR と無関係
      ignored: ["**/src/**", "**/target/**"],
    },
  },
});
