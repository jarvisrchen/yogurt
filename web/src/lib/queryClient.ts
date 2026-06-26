import { QueryClient } from "@tanstack/react-query";

/**
 * Singleton TanStack Query client for the SPA.
 *
 * Phase 5 (Plan 05-03): the only consumer is the Settings page, which
 * fetches `/api/settings` and mutates provider rows / keys.
 *
 * Defaults:
 * - `staleTime: 30_000` — keeps typing-fast UX while still picking up
 *   cross-tab changes within a reasonable window. Settings rarely mutate
 *   from outside the page; 30s is a comfortable middle ground.
 * - `refetchOnWindowFocus: false` — the focus-refetch default fires
 *   spurious requests every time the user tabs back, which would feel
 *   noisy on a localhost SPA.
 * - `retry: 1` on queries — one retry handles the rare cold-start race
 *   between Vite dev server boot and the axum API.
 * - `retry: 0` on mutations — mutations are user-initiated and idempotent
 *   surface (POST/PATCH); auto-retry would risk double-applying.
 */
export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      refetchOnWindowFocus: false,
      retry: 1,
    },
    mutations: { retry: 0 },
  },
});
