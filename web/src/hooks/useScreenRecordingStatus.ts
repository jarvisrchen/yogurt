import { useQuery } from "@tanstack/react-query";
import {
  fetchPermissions,
  permissionsKey,
  type PermissionStatus,
} from "../lib/api/audio";

export interface ScreenRecordingStatus {
  granted: boolean;
  status: PermissionStatus | undefined;
  owner: string | null;
  isLoading: boolean;
  error: Error | null;
}

export interface UseScreenRecordingStatusOptions {
  // Observers share permissionsKey, so the fastest interval wins app-wide. Callers outside onboarding pass a slower one.
  refetchIntervalMs?: number;
}

const DEFAULT_POLL_MS = 2_000;

export const screenRecordingKey = permissionsKey;

export function useScreenRecordingStatus(
  opts: UseScreenRecordingStatusOptions = {},
): ScreenRecordingStatus {
  const q = useQuery({
    queryKey: permissionsKey,
    queryFn: fetchPermissions,
    refetchInterval: opts.refetchIntervalMs ?? DEFAULT_POLL_MS,
    staleTime: 0,
  });

  const status = q.data?.screen_recording;
  const granted = status === "granted" || status === "not_required";

  return {
    granted,
    status,
    owner: q.data?.owner ?? null,
    isLoading: q.isLoading,
    error: q.error as Error | null,
  };
}
