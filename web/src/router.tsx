import {
  createBrowserRouter,
  Navigate,
  Outlet,
  type RouteObject,
} from "react-router";
import { StyleGuide } from "./routes/StyleGuide";
import { Meeting } from "./routes/Meeting";
import { MeetingPost } from "./routes/MeetingPost";
import { Settings } from "./routes/Settings";
import { Library } from "./routes/Library";
import { Welcome } from "./routes/Welcome";
import { useFirstRunRedirect } from "./hooks/useFirstRunRedirect";

/**
 * Top-level route table.
 *
 *   "/"                    — Phase 7 Library (sidebar + date-grouped meeting cards)
 *   "/starred"             — Library filtered to starred meetings
 *   "/welcome"             — Phase 7 Plan 07-04 onboarding flow (PRD §5.10)
 *   "/style-guide"         — Phase 1 design-system showcase
 *   "/meeting/new"         — Phase 4 bootstrap: POSTs /api/meetings then
 *                            redirects to /meeting/:id (replace).
 *   "/meeting/:id"         — Phase 3 in-meeting view (notes editor + dock).
 *   "/meeting/:id/post"    — Phase 4 hero post-meeting view (YogurtEditor +
 *                            EnhancingBanner + Re-enhance + Legend).
 *   "/settings"            — Phase 5 (Plan 05-03) Settings page.
 *
 * The Library route shape was kept at `/meeting/:id` instead of the
 * plan-suggested `/m/:id` to keep the Phase-3/4 surface stable. Same
 * conceptual link, smaller blast radius. (Auto-fix Rule 3.)
 *
 * `<Shell>` mounts `useFirstRunRedirect` at the top of the route tree —
 * the hook only redirects when `pathname === "/"`, so deep links to
 * `/settings` / `/meeting/:id` continue to work pre-onboarding. We use a
 * pathless wrapper route (no `path`) so every child renders inside the
 * shell without an extra URL segment.
 */

function Shell() {
  useFirstRunRedirect();
  return <Outlet />;
}

export const routes: RouteObject[] = [
  {
    element: <Shell />,
    children: [
      { path: "/", element: <Library /> },
      { path: "/starred", element: <Library starredOnly /> },
      { path: "/welcome", element: <Welcome /> },
      { path: "/style-guide", element: <StyleGuide /> },
      { path: "/meeting/new", element: <Meeting /> },
      { path: "/meeting/:id", element: <Meeting /> },
      { path: "/meeting/:id/post", element: <MeetingPost /> },
      { path: "/settings", element: <Settings /> },
      { path: "*", element: <Navigate to="/" replace /> },
    ],
  },
];

export const router = createBrowserRouter(routes);
