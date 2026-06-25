import { createBrowserRouter, type RouteObject } from "react-router";
import { App } from "./App";
import { StyleGuide } from "./routes/StyleGuide";

/**
 * Top-level route table. Phase 1 ships two routes:
 *   "/"            — the Phase-0 hello-world App (TipTap demo + health line)
 *   "/style-guide" — the design-system showcase (Task 3)
 *
 * Future phases will replace "/" with the library home (§5.9) and add
 * "/settings", "/welcome", "/meetings/:id", etc.
 */
export const routes: RouteObject[] = [
  { path: "/", element: <App /> },
  { path: "/style-guide", element: <StyleGuide /> },
];

export const router = createBrowserRouter(routes);
