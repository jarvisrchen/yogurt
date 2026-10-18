import { QueryClient } from "@tanstack/react-query";

// retry: 1 covers the cold-start race between the Vite dev server and axum.
// Mutations never retry so a POST/PATCH cannot double-apply.
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
