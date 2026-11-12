import React from "react";
import ReactDOM from "react-dom/client";
import "./styles.css";
if (new URLSearchParams(location.search).has("native")) {
  document.documentElement.classList.add("toolbar-document");
}
const entry = new URLSearchParams(location.search).has("recovery")
  ? import("./components/Recovery")
  : new URLSearchParams(location.search).has("workspace_start")
    ? import("./components/WorkspaceStart")
    : new URLSearchParams(location.search).has("preferences")
      ? import("./components/Preferences")
      : import("./App");
void entry.then(({ default: Root }) => {
  ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode>
      <Root />
    </React.StrictMode>,
  );
});
