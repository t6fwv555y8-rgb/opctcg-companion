import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { GlossaryProvider } from "./components/Glossary";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <GlossaryProvider>
      <App />
    </GlossaryProvider>
  </React.StrictMode>
);
