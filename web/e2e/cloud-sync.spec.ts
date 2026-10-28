import { test, expect, type Page } from "@playwright/test";

const SETTINGS = {
  general: {
    port: 7878,
    open_browser_on_start: true,
    audio_input_device: "",
    audio_echo_feature: false,
    audio_echo_output_device: "",
    audio_echo_enabled: false,
    audio_echo_buffer: 512,
    first_run_completed: true,
    stt_provider: "local",
    stt_model: "large-v3",
    meeting_detection: true,
    meeting_detection_focus: true,
    meeting_auto_stop: true,
  },
  providers: [
    {
      id: "p1",
      name: "Mock",
      base_url: "http://mock/v1",
      model: "m",
      is_active: true,
      created_at: 1,
      api_key_masked: "••••abcd",
      adapter: "http",
      cli_model: "",
    },
  ],
  presets: [],
  deepgram_key_masked: null,
  paths: {
    data_dir: "~/.yogurt",
    keys: "~/.yogurt/keys.json",
    notes: "~/.yogurt/notes",
  },
};

const PUSHED_AT = new Date(2026, 9, 23, 14, 59).getTime();

const connected = (over: Record<string, unknown> = {}) => ({
  connected: true,
  address: "https://richard-yogurt.vercel.app",
  email: "richard@example.com",
  machine_name: "Laptop 1",
  meetings: 46,
  not_pushed: 3,
  last_push: { at: PUSHED_AT, changed: 1, deleted: 0 },
  last_error: null,
  push_interval: "off",
  push: null,
  ...over,
});

const meetings = (n: number) =>
  Array.from({ length: n }, (_, i) => ({
    id: `019f0000-0000-7000-8000-${String(i).padStart(12, "0")}`,
    title: `Meeting ${i + 1}`,
    started_at: 1_700_000_000_000 - i * 3_600_000,
    ended_at: 1_700_000_000_000 - i * 3_600_000 + 1_800_000,
    notes_md: "- notes",
    enriched_md: null,
    transcript_json: "[]",
    starred: false,
    labels: [],
    created_at: "2026-08-13T00:00:00.000Z",
    updated_at: "2026-08-13T00:00:00.000Z",
  }));

interface State {
  status: Record<string, unknown>;
  statusFailure?: { status: number; code: string; error: string };
  posts: { path: string; body: unknown }[];
  pushError: { status: number; code: string; error: string } | null;
}

async function mockBackend(page: Page, state: State) {
  await page.routeWebSocket(/\/ws(\/|\?|$)/, () => {});
  await page.route("**/api/session-token", (r) => r.fulfill({ json: { token: "e2e-token" } }));
  await page.route(/\/api\/settings(\?|$)/, (r) => r.fulfill({ json: SETTINGS }));
  await page.route("**/api/audio/permission", (r) =>
    r.fulfill({ json: { screen_recording: "granted", microphone: "granted" } }),
  );
  await page.route(/\/api\/meetings(\?|$)/, (r) => {
    const q = new URL(r.request().url()).searchParams;
    const limit = Number(q.get("limit") ?? 25);
    const offset = Number(q.get("offset") ?? 0);
    return r.fulfill({
      json: { meetings: meetings(46).slice(offset, offset + limit), total: 46 },
    });
  });
  await page.route("**/api/labels", (r) => r.fulfill({ json: [] }));
  await page.route(/\/api\/cloud(\/[a-z-]+)?(\?|$)/, (r) => {
    const req = r.request();
    const path = new URL(req.url()).pathname;
    if (req.method() === "GET") {
      return state.statusFailure
        ? r.fulfill({ status: state.statusFailure.status, json: state.statusFailure })
        : r.fulfill({ json: state.status });
    }
    const body = req.postDataJSON();
    state.posts.push({ path, body });
    if (path === "/api/cloud/check") return r.fulfill({ json: { address: "https://richard-yogurt.vercel.app" } });
    if (path === "/api/cloud/connect") {
      if (body.password === "wrong") {
        return r.fulfill({ status: 403, json: { code: "not_owner", error: "This login does not own that Yogurt Cloud" } });
      }
      state.status = connected({ machine_name: body.machine_name, not_pushed: 46, last_push: null });
      return r.fulfill({ json: state.status });
    }
    if (path === "/api/cloud/push") {
      state.status = state.pushError
        ? connected({ last_error: { at: PUSHED_AT, error: state.pushError.error, code: state.pushError.code } })
        : connected({ not_pushed: 0, last_push: { at: PUSHED_AT, changed: 3, deleted: 0 } });
      return r.fulfill({ status: 202, json: { started: true } });
    }
    if (path === "/api/cloud/sign-in") {
      if (body.password === "nope") {
        return r.fulfill({ status: 401, json: { code: "bad_credentials", error: "Wrong email or password." } });
      }
      state.pushError = null;
      state.status = connected();
      return r.fulfill({ json: state.status });
    }
    if (path === "/api/cloud/push-interval") {
      state.status = { ...state.status, push_interval: body.interval };
      return r.fulfill({ json: state.status });
    }
    if (path === "/api/cloud/disconnect") {
      state.status = { connected: false };
      return r.fulfill({ json: state.status });
    }
    return r.fulfill({ status: 404, json: { error: "no route" } });
  });
}

const px = (loc: ReturnType<Page["locator"]>, prop: string) =>
  loc.evaluate((el, p) => getComputedStyle(el).getPropertyValue(p), prop);

for (const theme of ["light", "dark"] as const) {
  test(`cloud sync settings (${theme})`, async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.addInitScript((t) => localStorage.setItem("yogurt-theme", t), theme);
    const state: State = { status: { connected: false }, posts: [], pushError: null };
    await mockBackend(page, state);
    const shot = (name: string) =>
      page.screenshot({ path: `e2e/screenshots/cloud-${name}-${theme}.png`, fullPage: true });

    // A: nothing set up.
    await page.goto("/settings/cloud");
    await expect(page.getByRole("heading", { name: "Cloud sync" })).toBeVisible();
    await expect(page.getByRole("button", { name: /Cloud sync/ })).toContainText("new");
    await expect(page.getByRole("link", { name: /Open the guide/ })).toHaveAttribute(
      "href",
      "https://github.com/jarvisrchen/yogurt/blob/main/docs/CLOUD-SYNC.md",
    );
    await shot("A");

    // C: the connect form.
    await page.getByRole("button", { name: "Connect", exact: true }).click();
    await expect(page).toHaveURL(/\/settings\/cloud\/connect$/);
    await expect(page.getByRole("button", { name: "Connect", exact: true })).toBeDisabled();
    await expect(page.getByText("Cloud address")).toBeVisible();
    expect(await px(page.getByText("Cloud address"), "font-size")).toBe("9.5px");
    await page.getByLabel("Cloud address").fill("richard-yogurt.vercel.app");
    expect(await px(page.getByLabel("Cloud address"), "font-size")).toBe("11.5px");
    await page.getByLabel("Cloud address").blur();
    await expect(page.getByText("✓ Found a Yogurt Cloud")).toBeVisible();
    await page.getByLabel("Login email").fill("richard@example.com");
    await page.getByLabel("Login password").fill("hunter2hunter2");
    await page.getByLabel("Machine name").fill("Laptop 2");
    await shot("C");

    // A wrong login shows the server's typed error and clears the password field.
    await page.getByLabel("Login password").fill("wrong");
    await page.getByRole("button", { name: "Connect", exact: true }).click();
    await expect(page.getByRole("alert")).toContainText("This login does not own that Yogurt Cloud");
    await expect(page.getByLabel("Login password")).toHaveValue("");

    await page.getByLabel("Login password").fill("hunter2hunter2");
    await page.getByRole("button", { name: "Connect", exact: true }).click();
    await expect(page).toHaveURL(/\/settings\/cloud$/);
    expect(state.posts.at(-1)).toEqual({
      path: "/api/cloud/connect",
      body: {
        address: "richard-yogurt.vercel.app",
        email: "richard@example.com",
        password: "hunter2hunter2",
        machine_name: "Laptop 2",
      },
    });
    const stored = await page.evaluate(() => JSON.stringify([{ ...localStorage }, { ...sessionStorage }, location.href]));
    expect(stored).not.toContain("hunter2");

    // B: connected.
    state.status = connected();
    await page.reload();
    await expect(page.getByText("Connected", { exact: true })).toBeVisible();
    await expect(page.getByText("Signed in as richard@example.com")).toBeVisible();
    await expect(page.getByText("Laptop 1")).toBeVisible();
    await expect(page.getByText("3", { exact: true })).toBeVisible();
    await expect(page.getByText("Last push 10/23/26 2:59 PM · 1 uploaded")).toBeVisible();
    await expect(page.getByRole("img", { name: /QR code for richard-yogurt.vercel.app/ })).toBeVisible();
    await expect(page.getByRole("link", { name: /^Open/ })).toHaveAttribute("href", "https://richard-yogurt.vercel.app");
    await shot("B");

    // The mock's sizes, measured in the browser: Tailwind's same-property order must not let a base class win.
    const connectedPill = page.getByText("Connected", { exact: true });
    expect(await px(connectedPill, "font-size")).toBe("11px");
    expect(await px(connectedPill, "font-weight")).toBe("700");
    await expect(page.getByTestId("cloud-sync-pill")).toContainText("Cloud sync · on");
    await expect(page.getByText("Local-only")).toBeHidden();

    // Auto-push: off by default, and a choice posts and sticks.
    const auto = page.getByRole("radiogroup", { name: "Auto-push" });
    await expect(auto.getByRole("radio", { name: "Off" })).toBeChecked();
    await auto.getByRole("radio", { name: "15 min" }).click();
    await expect(auto.getByRole("radio", { name: "15 min" })).toBeChecked();
    expect(state.posts.at(-1)).toEqual({ path: "/api/cloud/push-interval", body: { interval: "15m" } });
    await page.waitForTimeout(300);
    await expect(auto.getByRole("radio", { name: "15 min" })).toBeChecked();
    await shot("B-autopush");

    // Push, then the stats and the last push result update.
    await page.getByRole("button", { name: "Push meetings" }).click();
    await expect(page.getByText("Last push 10/23/26 2:59 PM · 3 uploaded")).toBeVisible();

    // A revoked session shows the inline sign-in and pushing waits for it.
    state.pushError = { status: 401, code: "session_expired", error: "Your Cloud session expired. Sign in again." };
    await page.getByRole("button", { name: "Push meetings" }).click();
    await expect(page.getByRole("alert").first()).toContainText("Sign in again");
    await expect(page.getByRole("button", { name: "Push meetings" })).toBeDisabled();
    await shot("B-signin");
    // A wrong password keeps the prompt up with the server's message.
    await page.getByLabel("Cloud login password").fill("nope");
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await expect(page.getByText("Wrong email or password.")).toBeVisible();
    await expect(page.getByLabel("Cloud login password")).toHaveValue("");
    await page.getByLabel("Cloud login password").fill("hunter2hunter2");
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await expect(page.getByLabel("Cloud login password")).toBeHidden();
    await expect(page.getByRole("button", { name: "Push meetings" })).toBeEnabled();

    // Disconnect with the one-last-push option.
    await page.getByRole("button", { name: "Disconnect…" }).click();
    await page.getByLabel(/mark this machine/).check();
    await page.getByRole("button", { name: "Disconnect", exact: true }).click();
    await expect(page.getByText("Create a Yogurt Cloud")).toBeVisible();
    expect(state.posts.at(-1)).toEqual({ path: "/api/cloud/disconnect", body: { mark_deleted: true } });
  });

  test(`library header shows not pushed (${theme})`, async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.addInitScript((t) => localStorage.setItem("yogurt-theme", t), theme);
    const state: State = { status: connected(), posts: [], pushError: null };
    await mockBackend(page, state);
    await page.goto("/");
    const pill = page.getByRole("link", { name: "3 not pushed" });
    await expect(pill).toBeVisible();
    expect(await px(pill.locator("span"), "font-size")).toBe("10.5px");
    expect(await px(pill.locator("span"), "font-weight")).toBe("700");
    await expect(page.getByText("46 meetings · all on this Mac")).toBeVisible();
    await page.locator("header").filter({ hasText: "all on this Mac" }).screenshot({
      path: `e2e/screenshots/cloud-D-${theme}.png`,
    });
    await pill.click();
    await expect(page).toHaveURL(/\/settings\/cloud$/);

    // Nothing behind, nothing shown.
    state.status = connected({ not_pushed: 0 });
    await page.goto("/");
    await expect(page.getByText("46 meetings · all on this Mac")).toBeVisible();
    await expect(page.getByText("not pushed")).toBeHidden();
  });
}

test("a failing status keeps the cached section and offers Retry, and an unreadable connection offers Disconnect", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  const state: State = { status: connected(), posts: [], pushError: null };
  await mockBackend(page, state);
  await page.goto("/settings/cloud");
  await expect(page.getByText("Laptop 1")).toBeVisible();

  // A failed refetch keeps the last good data on screen.
  state.statusFailure = { status: 502, code: "network", error: "cloud unreachable" };
  await page.getByRole("button", { name: "Push meetings" }).click();
  await expect(page.getByRole("alert")).toContainText("cloud unreachable");
  await expect(page.getByText("Laptop 1")).toBeVisible();
  await expect(page.getByRole("button", { name: "Retry" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Disconnect", exact: true })).toBeHidden();

  state.statusFailure = { status: 500, code: "internal", error: "The Yogurt Cloud entry in keys.json is unreadable." };
  await page.reload();
  await expect(page.getByRole("alert")).toContainText("unreadable");
  await page.getByRole("button", { name: "Disconnect", exact: true }).click();
  await expect.poll(() => state.posts.at(-1)).toEqual({ path: "/api/cloud/disconnect", body: { mark_deleted: false } });
});
