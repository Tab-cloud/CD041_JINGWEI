import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import "./styles/tokens.css";
import "./styles/layout.css";
import "./styles/components.css";
import "./styles/panels.css";
import "./styles/responsive.css";
import { initializeTheme } from "./theme/theme";

const root = document.getElementById("root");
if (!root) throw new Error("Workbench root element is missing");

initializeTheme();

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
