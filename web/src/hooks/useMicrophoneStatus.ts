import { useQuery } from "@tanstack/react-query";
import {
  fetchPermissions,
  permissionsKey,
  type PermissionStatus,
} from "../lib/api/audio";

export interface MicrophoneStatus {
  granted: boolean;
  status: PermissionStatus | undefined;
  isLoading: boolean;
  error: Error | null;
}

export interface UseMicrophoneStatusOptions {
  // Observers share permissionsKey, so the fastest interval wins app-wide. Callers outside onboarding pass a slower one.
  refetchIntervalMs?: number;
}

const DEFAULT_POLL_MS = 2_000;

export function useMicrophoneStatus(
  opts: UseMicrophoneStatusOptions = {},
): MicrophoneStatus {
  const q = useQuery({
    queryKey: permissionsKey,
    queryFn: fetchPermissions,
    refetchInterval: opts.refetchIntervalMs ?? DEFAULT_POLL_MS,
    staleTime: 0,
  });

  const status = q.data?.microphone;
  const granted = status === "granted" || status === "not_required";

  return {
    granted,
    status,
    isLoading: q.isLoading,
    error: q.error as Error | null,
  };
}
