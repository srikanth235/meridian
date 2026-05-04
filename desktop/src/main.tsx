import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
// pi-web-ui ships precompiled Tailwind v4 utilities + preflight. We import
// it as a raw string and inject a single <style> tag so PostCSS (which is
// configured for our Tailwind v3 build) doesn't try to re-process the v4
// directives. The v4 sheet lands in document order BEFORE our v3 styles
// below, so our own preflight wins on shared selectors and chat-panel
// descendants resolve their (v3-absent) utility classes against the v4
// sheet.
import piWebUiCss from "@mariozechner/pi-web-ui/app.css?raw";
const piStyle = document.createElement("style");
piStyle.setAttribute("data-source", "pi-web-ui");
piStyle.textContent = piWebUiCss;
document.head.appendChild(piStyle);
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
