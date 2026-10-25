import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { bearerFetch } from "../session";

// The Cloud password is a request input only. It lives in React state until the request is sent and is never persisted.

export interface LastPush {
  at: number;
  changed: number;
  deleted: number;
}

/** The most recent failed cloud operation, cleared by the next success. */
export interface LastError {
  at: number;
  error: string;
  code: string;
}

export type CloudStatus =
  | { connected: false }
  | {
      connected: true;
      /** Origin such as `https://richard-yogurt.vercel.app`. */
      address: string;
      email: string;
      machine_name: string;
      meetings: number;
      not_pushed: number;
      last_push: LastPush | null;
      last_error: LastError | null;
    };

export type CloudConnected = Extract<CloudStatus, { connected: true }>;

export interface ConnectRequest {
  address: string;
  email: string;
  password: string;
  machine_name: string;
}

/** `code` is the server's stable error id, such as `session_expired` or `not_owner`. */
export class CloudError extends Error {
  code: string;
  constructor(message: string, code: string) {
    super(message);
    this.code = code;
  }
}

async function request<T>(method: "GET" | "POST", path: string, body?: unknown): Promise<T> {
  const res = await bearerFetch(path, {
    method,
    headers: { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await res.text();
  let parsed: { error?: unknown; code?: unknown } | undefined;
  try {
    parsed = text ? JSON.parse(text) : undefined;
  } catch {
    // Not JSON: the status line below is all there is to say.
  }
  if (!res.ok) {
    throw new CloudError(
      typeof parsed?.error === "string" ? parsed.error : `${res.status} ${res.statusText}`,
      typeof parsed?.code === "string" ? parsed.code : "unknown",
    );
  }
  if (parsed === undefined) throw new CloudError("The server sent an unreadable response.", "unknown");
  return parsed as T;
}

const post = <T>(path: string, body: unknown) => request<T>("POST", path, body);

export const cloudApi = {
  status: () => request<CloudStatus>("GET", "/api/cloud"),
  check: (address: string) => post<{ address: string }>("/api/cloud/check", { address }),
  connect: (req: ConnectRequest) => post<CloudStatus>("/api/cloud/connect", req),
  signIn: (password: string) => post<CloudStatus>("/api/cloud/sign-in", { password }),
  push: () => post<{ at: number; changed: number; deleted: number }>("/api/cloud/push", {}),
  disconnect: (markDeleted: boolean) => post<CloudStatus>("/api/cloud/disconnect", { mark_deleted: markDeleted }),
};

export const CLOUD_KEY = ["cloud"] as const;

export function useCloudStatus() {
  return useQuery({ queryKey: CLOUD_KEY, queryFn: cloudApi.status, retry: false });
}

/**
 * Every cloud mutation refetches the status when it settles, because a failed push records its error on the server.
 * `gcTime: 0` drops the variables, which can hold the Cloud password, as soon as nothing observes the mutation.
 */
export function useCloudMutation<V, R>(fn: (v: V) => Promise<R>) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: fn,
    gcTime: 0,
    onSettled: () => qc.invalidateQueries({ queryKey: CLOUD_KEY }),
  });
}
