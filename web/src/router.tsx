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
import { RecordingPill } from "./components/RecordingPill";
import { MeetingDetectedBanner } from "./components/MeetingDetectedBanner";
import { DownloadPill } from "./components/DownloadPill";
import { ModelDownloadDialog } from "./components/dialogs/ModelDownloadDialog";
import { useModelDownloadEvents } from "./hooks/useModelDownload";

// Pathless Shell wraps every route. useFirstRunRedirect only gates `/`, so deep links work before onboarding.
// The banner and recording pill can never show together: the server reports no detection while a recording is live.
// The download pill can sit beside either, so all three share one centered row.
function Shell() {
  useFirstRunRedirect();
  useModelDownloadEvents();
  return (
    <>
      <div className="fixed top-4 left-1/2 -translate-x-1/2 flex items-center gap-2 z-30">
        <RecordingPill />
        <MeetingDetectedBanner />
        <DownloadPill />
      </div>
      <ModelDownloadDialog />
      <Outlet />
    </>
  );
}

export const routes: RouteObject[] = [
  {
    element: <Shell />,
    children: [
      { path: "/", element: <Library /> },
      { path: "/starred", element: <Library starredOnly /> },
      { path: "/label/:labelId", element: <Library /> },
      { path: "/welcome", element: <Welcome /> },
      { path: "/style-guide", element: <StyleGuide /> },
      { path: "/meeting/new", element: <Meeting /> },
      { path: "/meeting/:id", element: <Meeting /> },
      { path: "/meeting/:id/post", element: <MeetingPost /> },
      { path: "/settings", element: <Navigate to="/settings/model" replace /> },
      { path: "/settings/:section", element: <Settings /> },
      { path: "*", element: <Navigate to="/" replace /> },
    ],
  },
];

export const router = createBrowserRouter(routes);
