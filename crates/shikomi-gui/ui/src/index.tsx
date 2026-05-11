import { render } from "solid-js/web";
import App from "./App";

const root = document.getElementById("root");
if (!(root instanceof HTMLElement)) {
  throw new Error("Root element not found. Check that index.html has <div id=\"root\">.");
}
render(() => <App />, root);
