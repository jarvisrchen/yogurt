import "@fontsource/manrope/400.css";
import "@fontsource/manrope/500.css";
import "@fontsource/manrope/600.css";
import "@fontsource/manrope/700.css";
import "@fontsource/manrope/800.css";
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/500.css";
import "@fontsource/jetbrains-mono/600.css";

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { initTheme } from "@web/lib/theme";
import { App } from "./App";
import "./index.css";

initTheme();

// The installed app's system bars follow the page background, which is the paper token.
const themeColor = document.querySelector<HTMLMetaElement>('meta[name="theme-color"]')!;
const syncThemeColor = () => (themeColor.content = getComputedStyle(document.documentElement).getPropertyValue("--color-paper").trim());
new MutationObserver(syncThemeColor).observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
syncThemeColor();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
