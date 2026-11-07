import { invoke } from "@tauri-apps/api/core";
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import UninstallApp from "./pages/Uninstall";
import "./index.css";

   
                                                                          
                                                
  
                               
                              
                    
                                           
                                               
   
function installContextMenuGuard() {
  document.addEventListener(
    "contextmenu",
    (event) => {
      const target = event.target;
      if (!(target instanceof Element)) {
        event.preventDefault();
        return;
      }
      const allow = target.closest(
        [
          "input",
          "textarea",
          "select",
          "[contenteditable='true']",
          "[contenteditable='']",
          "[data-allow-context-menu]",
          ".select-text",
        ].join(","),
      );
      if (allow) return;
      event.preventDefault();
      event.stopPropagation();
    },
    { capture: true },
  );
}

installContextMenuGuard();

async function boot() {
  let uninstall = false;
  try {
    uninstall = await invoke<boolean>("is_uninstall_mode");
  } catch {
    uninstall = false;
  }
  ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode>{uninstall ? <UninstallApp /> : <App />}</React.StrictMode>,
  );
}

void boot();
