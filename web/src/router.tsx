import { createBrowserRouter, Navigate, type RouteObject } from "react-router";
import { StyleGuide } from "./routes/StyleGuide";
import { Meeting } from "./routes/Meeting";
import { MeetingPost } from "./routes/MeetingPost";
import { Settings } from "./routes/Settings";
import { Library } from "./routes/Library";

/**
 * Top-level route table.
 *
 *   "/"                    — Phase 7 Library (sidebar + date-grouped meeting cards)
 *   "/starred"             — Placeholder redirect to "/" (UI surface ships v1.1)
 *   "/style-guide"         — Phase 1 design-system showcase
 *   "/meeting/new"         — Phase 4 bootstrap: POSTs /api/meetings then
 *                            redirects to /meeting/:id (replace).
 *   "/meeting/:id"         — Phase 3 in-meeting view (notes editor + dock).
 *   "/meeting/:id/post"    — Phase 4 hero post-meeting view (YogurtEditor +
 *                            EnhancingBanner + Re-enhance + Legend).
 *   "/settings"            — Phase 5 (Plan 05-03) Settings page.
 *   "/welcome"             — Phase 7 Plan 07-03 onboarding (stub for now —
 *                            redirects to "/" until that plan lands).
 *
 * Phase 7 (Plan 07-01) demotes the Phase-3 stub App.tsx and promotes the
 * Library to "/". The plan suggested route shape was `/m/:id`; we kept the
 * existing `/meeting/:id` to minimize regression surface — same conceptual
 * link, smaller blast radius. (Auto-fix Rule 3.)
 */
export const routes: RouteObject[] = [
  { path: "/", element: <Library /> },
  { path: "/starred", element: <Navigate to="/" replace /> },
  { path: "/welcome", element: <Navigate to="/" replace /> },
  { path: "/style-guide", element: <StyleGuide /> },
  { path: "/meeting/new", element: <Meeting /> },
  { path: "/meeting/:id", element: <Meeting /> },
  { path: "/meeting/:id/post", element: <MeetingPost /> },
  { path: "/settings", element: <Settings /> },
  { path: "*", element: <Navigate to="/" replace /> },
];

export const router = createBrowserRouter(routes);
