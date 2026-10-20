// 主题预置: 渲染前同步设置, 避免闪烁并保证 localStorage 持久化生效
(() => {
  const dark = localStorage.getItem("theme") !== "light";
  document.documentElement.classList.toggle("dark", dark);
})();

import React from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import App from "./App";
import { LogProvider } from "@/hooks/useUdpLogs";
import "@/styles/globals.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <BrowserRouter>
      <LogProvider>
        <App />
      </LogProvider>
    </BrowserRouter>
  </React.StrictMode>,
);
