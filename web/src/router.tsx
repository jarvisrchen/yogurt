import { createBrowserRouter, type RouteObject } from "react-router";
import { App } from "./App";
import { StyleGuide } from "./routes/StyleGuide";
import { Meeting } from "./routes/Meeting";
import { MeetingPost } from "./routes/MeetingPost";

/**
 * Top-level route table.
 *
 *   "/"                    — Phase 3 library stub + "Open a new meeting →" CTA
 *   "/style-guide"         — Phase 1 design-system showcase
 *   "/meeting/new"         — Phase 4 bootstrap: POSTs /api/meetings then
 *                            redirects to /meeting/:id (replace).
 *   "/meeting/:id"         — Phase 3 in-meeting view (notes editor + dock).
 *   "/meeting/:id/post"    — Phase 4 hero post-meeting view (YogurtEditor +
 *                            EnhancingBanner + Re-enhance + Legend).
 *
 * Phase 7 reorganizes the App.tsx toggle into a proper library home; for
 * now the toggle CTA navigates to /meeting/new.
 */
export const routes: RouteObject[] = [
  { path: "/", element: <App /> },
  { path: "/style-guide", element: <StyleGuide /> },
  { path: "/meeting/new", element: <Meeting /> },
  { path: "/meeting/:id", element: <Meeting /> },
  { path: "/meeting/:id/post", element: <MeetingPost /> },
];

export const router = createBrowserRouter(routes);
