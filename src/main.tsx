import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClientProvider } from "@tanstack/react-query";
import App from "./App";
import { queryClient } from "./cloud/api";
import { IS_MAC } from "./lib/hotkeys";
import "./index.css";

// Key labels switch to the system font on macOS, where ⌘ ⌥ ⌃ ⇧ live.
if (IS_MAC) document.documentElement.dataset.platform = "mac";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </React.StrictMode>,
);
