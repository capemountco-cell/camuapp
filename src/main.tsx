import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./App";
import { Popover } from "./Popover";
import { isPopover } from "./lib/native";
import "./styles.css";

const root = document.getElementById("root");
if (!root) throw new Error("No #root element in index.html.");

createRoot(root).render(
  <StrictMode>
    {isPopover() ? <Popover /> : <App />}
  </StrictMode>,
);
