import React from "react";
import ReactDOM from "react-dom/client";
import SetupApp from "./pages/Setup";
import "./index.css";

document.addEventListener(
  "contextmenu",
  (event) => {
    event.preventDefault();
    event.stopPropagation();
  },
  { capture: true },
);

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <SetupApp />
  </React.StrictMode>,
);
