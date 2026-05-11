import type { Component } from "solid-js";

// Sub-A: 骨格のみ。UI コンポーネントは Sub-C (#96) で実装する。
// 設計根拠: docs/features/shikomi-gui/feature-spec.md UC-GUI-001
const App: Component = () => {
  return (
    <main style={{ padding: "2rem", "font-family": "system-ui, sans-serif" }}>
      <h1>shikomi</h1>
      <p>daemon に接続中...</p>
    </main>
  );
};

export default App;
