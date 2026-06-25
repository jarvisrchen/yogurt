# Yogurt v1 — Phase 1: Design System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the throwaway Phase-0 styling with the proper Yogurt design system. Wire Tailwind 4 with the brand tokens from PRD §16 via the `@theme` directive, ship the three font families (Instrument Serif, Hanken Grotesk, JetBrains Mono) via `@fontsource/*`, build the core component primitives (`Logo`, `Button`, `Pill`, `RecordingBadge`, `Card`, `BrowserChrome`), and stand up a `/style-guide` route that renders every token + component as a visual proof that the system is wired correctly.

**Architecture:** Phase 0 already scaffolded React 19 + Vite 6 + Tailwind 4 with a placeholder `index.css`. Phase 1 expands `index.css` with a real `@theme` block, installs the three `@fontsource/*` packages as side-effect imports in `main.tsx`, adds React Router 7 with two routes (`/` for the existing Phase-0 hello-world, `/style-guide` for the design-system showcase), and creates six reusable components in `web/src/components/`. No backend changes. No new features beyond components and the showcase route. The "Blueberry-only" theme decision from PRD §15 means we ship one token block, not three — Strawberry and Matcha-dark are explicitly out of scope.

**Tech Stack:** Tailwind 4 (`@theme` directive — no separate `tailwind.config.ts`) · React 19 · Vite 6 · TypeScript 5.6 · Vitest 2 · @testing-library/react 16 · React Router 7 (`react-router` v7 — note the package was renamed from `react-router-dom`) · `@fontsource/instrument-serif` · `@fontsource/hanken-grotesk` · `@fontsource/jetbrains-mono`

**Reference:** `docs/PRD.md` §16 (Brand & Visual Design System — full source of truth), §5.9 (library — needs Logo, Pill, Card), §5.10 (onboarding — needs BrowserChrome, Card, Button), §5.11 (empty/error states — needs Pill, Button, motion tokens). Phase 0 plan: `docs/superpowers/plans/2026-06-25-yogurt-phase-0-skeleton.md`.

**Dependencies on prior phases:** Phase 0 must be complete — `cargo test --workspace` and `pnpm --dir web test` both pass, `web/src/index.css` exists with `@import "tailwindcss";`, `web/src/App.tsx` renders the TipTap demo, the Cargo workspace + axum + Vite scaffold is runnable. **Do not redo any Phase-0 setup.**

**Out of scope (deferred to later phase plans):**
- Settings page UI / Model section cards (Phase 5)
- Library home view + sidebar layout (Phase 7)
- Onboarding 3-step flow (Phase 7)
- Empty / error / enhancing state screens (Phase 7; Phase 4 wires the enhancing motion)
- The TipTap `aiGrey` mark + `transcriptTs` deep-link node (Phase 4)
- Tab group component (Phase 4 — only needed by post-meeting view)
- Strawberry + Matcha-dark themes (deferred per PRD §15, may be a v1.x add-on)
- Real icon system (Phase 7 picks Lucide / Phosphor; this phase uses inline SVGs and Unicode glyphs)
- Light/dark mode toggle (single Blueberry theme in v1)

---

## File structure produced by this phase

```
yogurt/
├── web/
│   ├── package.json                          # MODIFY · add react-router + @fontsource/*
│   ├── pnpm-lock.yaml                        # AUTO-UPDATED
│   ├── src/
│   │   ├── main.tsx                          # MODIFY · mount RouterProvider, import fonts
│   │   ├── App.tsx                           # MODIFY · use Button + Card components
│   │   ├── App.test.tsx                      # MODIFY · re-anchor selectors to new markup
│   │   ├── index.css                         # MODIFY · add @theme block + font-face fallbacks
│   │   ├── router.tsx                        # NEW · createBrowserRouter with / and /style-guide
│   │   ├── components/
│   │   │   ├── Logo.tsx                      # NEW · spoon-and-swirl SVG, sizable
│   │   │   ├── Logo.test.tsx                 # NEW
│   │   │   ├── Button.tsx                    # NEW · primary/secondary/ghost variants
│   │   │   ├── Button.test.tsx               # NEW
│   │   │   ├── Pill.tsx                      # NEW · base Pill + RecordingBadge + ProviderChip
│   │   │   ├── Pill.test.tsx                 # NEW
│   │   │   ├── Card.tsx                      # NEW · base surface with optional border/shadow
│   │   │   ├── Card.test.tsx                 # NEW
│   │   │   ├── BrowserChrome.tsx             # NEW · fake-Safari wrapper for mockups
│   │   │   └── BrowserChrome.test.tsx        # NEW
│   │   └── routes/
│   │       └── StyleGuide.tsx                # NEW · renders every token + component
└── docs/
    └── superpowers/plans/
        └── 2026-06-25-yogurt-phase-1-design-system.md  # THIS FILE
```

**Why this split:** `components/` holds reusable primitives that later phases (library, settings, onboarding) compose. `routes/` is a thin layer that owns page-level layout; `StyleGuide.tsx` is the first route and serves double duty as living documentation of the tokens. `router.tsx` is split out of `main.tsx` so it can be tested in isolation later (Phase 7 will add a `<MemoryRouter>` wrapper for library tests).

---

## Test conventions established in this phase

- **Component tests:** colocated as `<Component>.test.tsx` beside the source. Use Vitest + `@testing-library/react`. Each test file establishes the standard pattern: import the component, render it in a single `it("renders …")` smoke test, then add behavior tests as needed (variant rendering, click handlers, `data-testid` presence for visual-only elements).
- **Naming:** `it('<does thing>', ...)` — same as Phase 0.
- **Render utility:** for components that depend on Router (none in this phase yet — `Logo`, `Button`, `Pill`, `Card`, `BrowserChrome` are all router-agnostic), no wrapper needed. Phase 7's library/sidebar tests will introduce a `renderWithRouter` helper.
- **Visual validation:** the `/style-guide` route is the human-validation surface — manual eyeball check, not automated screenshot diffing (Playwright + visual regression is Phase 9+ polish).
- **No new Rust tests.** This phase is frontend-only; `cargo test --workspace` should still pass unmodified at the end.

---

## Phase 1 task list

10 tasks. Each task ends with a commit. Approximate sequence: ~10–14 hours of focused work (1.5 days). The TDD loop is: **write failing test → run/fail → implement → run/pass → commit.**

---

### Task 1.1 · Tailwind 4 `@theme` block with PRD §16 tokens

**Files:**
- Modify: `web/src/index.css`

- [ ] **Step 1: Inspect current state.**

Run: `cat web/src/index.css`
Expected (from Phase 0 Task 0.5 Step 6):

```css
@import "tailwindcss";

:root { color-scheme: light; }
body { margin: 0; font-family: system-ui, -apple-system, "Segoe UI", sans-serif; background: #FBF7EF; color: #211D18; }
```

The hardcoded `#FBF7EF` and `#211D18` are the placeholders we're replacing with real tokens.

- [ ] **Step 2: Replace `web/src/index.css` with the full `@theme` block.**

```css
@import "tailwindcss";

/* ────────────────────────────────────────────────────────────────────────────
 * Yogurt design tokens — PRD §16 (Blueberry theme, the default and only v1 theme).
 * Strawberry and Matcha-dark themes are documented in §16.2 but deferred
 * (see §15 — "Blueberry only in v1").
 *
 * The `@theme` directive is Tailwind 4's CSS-first config replacement for
 * tailwind.config.ts. Every variable here becomes a Tailwind utility:
 *   --color-paper   → bg-paper, text-paper, border-paper, etc.
 *   --font-serif    → font-serif
 *   --radius-card   → rounded-card
 *   --shadow-card   → shadow-card
 *   --ease-slide    → ease-slide (used in transition-timing)
 *   --animate-recpulse → animate-recpulse
 * ──────────────────────────────────────────────────────────────────────────── */

@theme {
  /* ─── Color (PRD §16.2) ──────────────────────────────────────────────── */
  --color-paper:   #FBF7EF;  /* app background, hero surfaces             */
  --color-card:    #FFFFFF;  /* cards, surfaces over paper                */
  --color-ink:     #211D18;  /* user notes, headings, primary text        */
  --color-grey:    #A89F90;  /* AI-added text, secondary captions         */
  --color-line:    #EBE3D5;  /* borders, dividers                         */
  --color-blue:    #5B4FC7;  /* primary blueberry — buttons, transcript   */
  --color-blsoft:  #ECE9FB;  /* soft blueberry — active-nav, pill bg      */
  --color-straw:   #E07A66;  /* recording indicator, error accent         */
  --color-strsoft: #FBE6E0;  /* soft strawberry — error/warning bg        */
  --color-matcha:  #5E9E73;  /* local-only / privacy / success            */
  --color-mtsoft:  #E7F0E8;  /* soft matcha — local-mode badges           */
  --color-mut:     #8A8174;  /* muted text on cards                       */

  /* ─── Type families (PRD §16.3) ──────────────────────────────────────── */
  --font-serif: "Instrument Serif", ui-serif, Georgia, serif;
  --font-sans:  "Hanken Grotesk", ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif;
  --font-mono:  "JetBrains Mono", ui-monospace, "SFMono-Regular", Menlo, monospace;

  /* ─── Border radius (PRD §16.4) ──────────────────────────────────────── */
  --radius-chip:   6px;    /* small chip                                    */
  --radius-button: 9px;    /* buttons, inputs                               */
  --radius-card:   14px;   /* cards                                         */
  --radius-pill:   999px;  /* pills, recording badge                        */

  /* ─── Elevation (PRD §16.4) ──────────────────────────────────────────── */
  --shadow-card:   0 2px 6px rgba(40, 30, 15, 0.08);
  --shadow-pop:    0 12px 30px -10px rgba(40, 30, 15, 0.22);
  --shadow-window: 0 26px 60px -28px rgba(40, 30, 15, 0.4);
  --shadow-button-blue: 0 2px 8px rgba(91, 79, 199, 0.3);

  /* ─── Motion easings (PRD §16.5) ─────────────────────────────────────── */
  --ease-pop:   cubic-bezier(0, 0, 0.2, 1);          /* popUp 260ms        */
  --ease-slide: cubic-bezier(0.2, 0.7, 0.2, 1);      /* slideInRight 340ms */

  /* ─── Animations (PRD §16.5) ─────────────────────────────────────────── */
  --animate-recpulse:  recpulse 1.4s ease-in-out infinite;
  --animate-blink:     blink    1.0s step-end infinite;
  --animate-shimmer:   shimmer  1.25s linear infinite;
  --animate-wave:      wave     1.0s ease-in-out infinite;
  --animate-float:     float    3.5s ease-in-out infinite;
  --animate-pop-up:    pop-up   260ms cubic-bezier(0, 0, 0.2, 1);
  --animate-slide-in-right: slide-in-right 340ms cubic-bezier(0.2, 0.7, 0.2, 1);
}

/* ─── Keyframes (referenced by --animate-* above) ─────────────────────────── */
@keyframes recpulse {
  0%, 100% { opacity: 1;   transform: scale(1);    }
  50%      { opacity: 0.55; transform: scale(1.15); }
}
@keyframes blink {
  0%, 49%   { opacity: 1; }
  50%, 100% { opacity: 0; }
}
@keyframes shimmer {
  0%   { background-position: -200% 0; }
  100% { background-position:  200% 0; }
}
@keyframes wave {
  0%, 100% { transform: scaleY(0.6); }
  50%      { transform: scaleY(1.0); }
}
@keyframes float {
  0%, 100% { transform: translateY(0px); }
  50%      { transform: translateY(-6px); }
}
@keyframes pop-up {
  0%   { opacity: 0; transform: scale(0.96) translateY(4px); }
  100% { opacity: 1; transform: scale(1)    translateY(0);    }
}
@keyframes slide-in-right {
  0%   { transform: translateX(100%); opacity: 0; }
  100% { transform: translateX(0);    opacity: 1; }
}

/* ─── Base resets ─────────────────────────────────────────────────────────── */
:root { color-scheme: light; }

html, body {
  margin: 0;
  background: var(--color-paper);
  color: var(--color-ink);
  font-family: var(--font-sans);
  font-size: 15px;
  line-height: 1.5;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}
```

- [ ] **Step 3: Verify Tailwind picks up the theme.**

Run: `pnpm --dir web build`
Expected: succeeds. The CSS chunk should now contain the variable declarations and tailwind-generated utilities `bg-paper`, `text-ink`, `font-serif`, `rounded-card`, etc. (Don't grep blindly — Tailwind 4 only emits classes actually used; we'll prove it works in Task 1.4 when components use them.)

Run: `pnpm --dir web dev` in one terminal, then `open http://localhost:5173`.
Expected: existing Phase 0 page still renders (the body bg should still look cream, text still ink — same as Phase 0, just now sourced from the token variables).

Stop Vite (Ctrl-C).

- [ ] **Step 4: Commit.**

```bash
git add web/src/index.css
git commit -m "feat(web): add tailwind 4 @theme block with PRD §16 design tokens"
```

---

### Task 1.2 · Install `@fontsource/*` packages and wire fonts

**Files:**
- Modify: `web/package.json` (add 3 deps)
- Modify: `web/src/main.tsx` (add 5 font side-effect imports)

- [ ] **Step 1: Add the three font packages.**

Run:
```bash
pnpm --dir web add @fontsource/instrument-serif @fontsource/hanken-grotesk @fontsource/jetbrains-mono
```

Expected: three new entries under `dependencies` in `web/package.json`, lockfile updated.

- [ ] **Step 2: Import font weights as side-effect imports in `web/src/main.tsx`.**

Each `@fontsource/<family>/<weight>.css` import registers a `@font-face` rule in the bundle. Per PRD §16.3:
- Instrument Serif: 400 + 400 italic
- Hanken Grotesk: 400, 500, 600, 700, 800
- JetBrains Mono: 400, 500, 600

Modify the top of `web/src/main.tsx` — keep the existing `StrictMode`, `createRoot`, `App`, `index.css` imports, but prepend the font imports above `./index.css`:

```tsx
// Font registration — side-effect imports register @font-face rules.
// Order matters only for CSS dedup; functionally all are independent.
import "@fontsource/instrument-serif/400.css";
import "@fontsource/instrument-serif/400-italic.css";
import "@fontsource/hanken-grotesk/400.css";
import "@fontsource/hanken-grotesk/500.css";
import "@fontsource/hanken-grotesk/600.css";
import "@fontsource/hanken-grotesk/700.css";
import "@fontsource/hanken-grotesk/800.css";
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/500.css";
import "@fontsource/jetbrains-mono/600.css";

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./index.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>
);
```

(We will further modify `main.tsx` in Task 1.7 to mount the router — but we want fonts wired now so the next task's components render with the right type immediately.)

- [ ] **Step 3: Smoke-check that fonts load.**

Run: `pnpm --dir web dev`
Open `http://localhost:5173` and open DevTools → Network → filter "font". Reload.
Expected: ~10 `.woff2` requests, all 200 OK. The headline should now render in Hanken Grotesk (or fall back to the system stack if you haven't built a serif-class element yet — that's fine, we'll add one in Task 1.6).

Open DevTools → Console. There should be **no "FontFace … failed to load"** warnings.

Stop Vite (Ctrl-C).

- [ ] **Step 4: Commit.**

```bash
git add web/package.json web/pnpm-lock.yaml web/src/main.tsx
git commit -m "feat(web): add @fontsource for instrument-serif, hanken-grotesk, jetbrains-mono"
```

---

### Task 1.3 · `Logo` component (the spoon-and-swirl SVG)

**Files:**
- Create: `web/src/components/Logo.tsx`
- Create: `web/src/components/Logo.test.tsx`

The exact SVG is lifted from `yogurt-app-design/project/Yogurt Design Board.dc.html` line 32. It's a 44×44 viewBox with three primitives: a blueberry circle, a white spoon-curve `path`, and a small strawberry `circle` at the tip of the spoon. Renders cleanly from 19px (favicon-ish) up to 60px (hero) per PRD §16.1.

- [ ] **Step 1: Write the failing test.**

Create `web/src/components/Logo.test.tsx`:

```tsx
import { describe, it, expect } from "vitest";
import { render } from "@testing-library/react";
import { Logo } from "./Logo";

describe("Logo", () => {
  it("renders an SVG with the spoon-and-swirl mark", () => {
    const { container } = render(<Logo size={44} />);
    const svg = container.querySelector("svg");
    expect(svg).not.toBeNull();
    expect(svg!.getAttribute("viewBox")).toBe("0 0 44 44");
    expect(svg!.getAttribute("width")).toBe("44");
    expect(svg!.getAttribute("height")).toBe("44");
  });

  it("uses the brand colors (blueberry + strawberry)", () => {
    const { container } = render(<Logo />);
    const fills = Array.from(container.querySelectorAll("[fill]")).map((el) =>
      el.getAttribute("fill")
    );
    expect(fills).toContain("#5B4FC7"); // blueberry
    expect(fills).toContain("#E07A66"); // strawberry dot
  });

  it("defaults to 44px when no size is provided", () => {
    const { container } = render(<Logo />);
    const svg = container.querySelector("svg")!;
    expect(svg.getAttribute("width")).toBe("44");
    expect(svg.getAttribute("height")).toBe("44");
  });

  it("forwards an aria-label when provided", () => {
    const { getByLabelText } = render(<Logo ariaLabel="Yogurt" />);
    expect(getByLabelText("Yogurt")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run — expect compile failure.**

Run: `pnpm --dir web test -- Logo`
Expected: `Cannot find module './Logo'` or similar.

- [ ] **Step 3: Implement `web/src/components/Logo.tsx`.**

```tsx
interface LogoProps {
  /** Render size in CSS pixels. Default 44 (matches the brand mark). */
  size?: number;
  /** Accessible name for the SVG. Omit for decorative use. */
  ariaLabel?: string;
  /** Optional className for spacing/positioning. */
  className?: string;
}

/**
 * Yogurt logo — the "spoon & swirl" mark (PRD §16.1).
 * Blueberry circle, white spoon-curve, strawberry dot at the spoon tip.
 * The viewBox is locked to 44×44; pass `size` to scale.
 */
export function Logo({ size = 44, ariaLabel, className }: LogoProps) {
  const a11yProps = ariaLabel
    ? { role: "img" as const, "aria-label": ariaLabel }
    : { "aria-hidden": true as const };

  return (
    <svg
      viewBox="0 0 44 44"
      width={size}
      height={size}
      className={className}
      style={{ flex: "none" }}
      {...a11yProps}
    >
      <circle cx="22" cy="22" r="22" fill="#5B4FC7" />
      <path
        d="M13 31 C 13 22, 31 25, 31 17 C 31 11, 19 12.5, 19 19"
        fill="none"
        stroke="#fff"
        strokeWidth="3.4"
        strokeLinecap="round"
      />
      <circle cx="19" cy="19" r="1.7" fill="#E07A66" />
    </svg>
  );
}
```

- [ ] **Step 4: Run — expect PASS.**

Run: `pnpm --dir web test -- Logo`
Expected: `4 passed`.

- [ ] **Step 5: Commit.**

```bash
git add web/src/components/Logo.tsx web/src/components/Logo.test.tsx
git commit -m "feat(web): add Logo component (spoon-and-swirl SVG mark)"
```

---

### Task 1.4 · `Button` component (primary / secondary / ghost variants)

**Files:**
- Create: `web/src/components/Button.tsx`
- Create: `web/src/components/Button.test.tsx`

Three variants per PRD §16.6:
- **Primary** — blueberry bg, white text, 13.5px Hanken 600, `9px` radius, blueberry-tinted shadow. Use for "New meeting", "End meeting" (ink ver. — see note), "Take me to my meetings →", "Re-enhance", "Generate".
- **Secondary** — white bg, ink text, `1px solid #D9D0C0` border, same dimensions. Use for "Enhance", "Restart Yogurt", "Cancel" (in card context).
- **Ghost** — transparent, muted-grey text. Use for unobtrusive cancels.

**Note:** PRD §16.6 mentions "End meeting (ink version)" — that's a fourth, ink-on-cream variant used during the live meeting view (Phase 4). We do not need it for v1.0 of the design system; Phase 4 can extend `Button` with a `variant="ink"` when it needs it. Document this in a comment, do not implement now.

- [ ] **Step 1: Write the failing test.**

Create `web/src/components/Button.test.tsx`:

```tsx
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { Button } from "./Button";

describe("Button", () => {
  it("renders children as label text", () => {
    render(<Button>New meeting</Button>);
    expect(screen.getByRole("button", { name: /new meeting/i })).toBeInTheDocument();
  });

  it("defaults to the primary variant", () => {
    render(<Button>Go</Button>);
    const btn = screen.getByRole("button");
    expect(btn.className).toMatch(/bg-blue/);
    expect(btn.className).toMatch(/text-white/);
  });

  it("renders secondary variant with border", () => {
    render(<Button variant="secondary">Cancel</Button>);
    const btn = screen.getByRole("button");
    expect(btn.className).toMatch(/bg-card/);
    expect(btn.className).toMatch(/border/);
    expect(btn.className).toMatch(/text-ink/);
  });

  it("renders ghost variant as transparent + muted", () => {
    render(<Button variant="ghost">Dismiss</Button>);
    const btn = screen.getByRole("button");
    expect(btn.className).toMatch(/bg-transparent/);
    expect(btn.className).toMatch(/text-mut/);
  });

  it("fires onClick", () => {
    const onClick = vi.fn();
    render(<Button onClick={onClick}>Click</Button>);
    fireEvent.click(screen.getByRole("button"));
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("respects disabled", () => {
    const onClick = vi.fn();
    render(
      <Button disabled onClick={onClick}>
        nope
      </Button>
    );
    fireEvent.click(screen.getByRole("button"));
    expect(onClick).not.toHaveBeenCalled();
    expect(screen.getByRole("button")).toBeDisabled();
  });

  it("supports type='submit'", () => {
    render(<Button type="submit">Submit</Button>);
    expect(screen.getByRole("button")).toHaveAttribute("type", "submit");
  });
});
```

- [ ] **Step 2: Run — expect failure.**

Run: `pnpm --dir web test -- Button`
Expected: cannot resolve `./Button`.

- [ ] **Step 3: Implement `web/src/components/Button.tsx`.**

```tsx
import { type ButtonHTMLAttributes, type ReactNode } from "react";

type Variant = "primary" | "secondary" | "ghost";

interface ButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "className"> {
  variant?: Variant;
  children: ReactNode;
  /** Optional extra Tailwind classes appended after the variant classes. */
  className?: string;
}

const BASE = [
  "inline-flex",
  "items-center",
  "justify-center",
  "gap-2",
  "px-4",
  "py-2",
  "rounded-button",
  "font-sans",
  "text-[13.5px]",
  "font-semibold",
  "leading-none",
  "transition-colors",
  "transition-shadow",
  "duration-150",
  "ease-out",
  "select-none",
  "disabled:opacity-50",
  "disabled:cursor-not-allowed",
].join(" ");

const VARIANT: Record<Variant, string> = {
  // Blueberry button with branded shadow. Hover darkens via opacity overlay
  // since Tailwind 4 doesn't expose a 'blue-600'-style derived shade without
  // explicit token; opacity-90 on hover is consistent with the design board.
  primary:
    "bg-blue text-white shadow-[0_2px_8px_rgba(91,79,199,0.3)] hover:opacity-90 active:opacity-100",
  // Cream card surface with a slightly warmer line color than --line, per §16.6.
  secondary:
    "bg-card text-ink border border-[#D9D0C0] hover:bg-[#F8F2E5] active:bg-[#F1E9D7]",
  // Transparent, used inside cards where any background would compete.
  ghost: "bg-transparent text-mut hover:text-ink hover:bg-blsoft/50",
};

/**
 * Yogurt brand button (PRD §16.6).
 *
 * Variants:
 *   - "primary"   — blueberry, white text. New meeting / End meeting / CTAs.
 *   - "secondary" — white card, ink text, hairline border. Cancel / Restart.
 *   - "ghost"     — transparent, muted text. Subtle dismissals.
 *
 * Future variants (deferred to Phase 4): "ink" — ink-on-cream End-meeting
 * style used in the live meeting top bar.
 */
export function Button({
  variant = "primary",
  children,
  className = "",
  type = "button",
  ...rest
}: ButtonProps) {
  const cls = `${BASE} ${VARIANT[variant]} ${className}`.trim();
  return (
    <button type={type} className={cls} {...rest}>
      {children}
    </button>
  );
}
```

- [ ] **Step 4: Run — expect PASS.**

Run: `pnpm --dir web test -- Button`
Expected: `7 passed`.

- [ ] **Step 5: Commit.**

```bash
git add web/src/components/Button.tsx web/src/components/Button.test.tsx
git commit -m "feat(web): add Button component with primary/secondary/ghost variants"
```

---

### Task 1.5 · `Pill` component + `RecordingBadge` + `ProviderChip` variants

**Files:**
- Create: `web/src/components/Pill.tsx`
- Create: `web/src/components/Pill.test.tsx`

The `Pill` is the generic 999-radius element used for badges and chips throughout the app. From PRD it appears as:
- The "Local-only · on" matcha-soft pill in the library sidebar (§5.9)
- The recording badge with pulsing strawberry dot + timer (§16.6)
- The `⌘K` keyboard hint badge inside the Ask pill (§5.4)
- The provider chip rows (Ollama / LM Studio / OpenRouter, §5.6)
- The lilac "✨ AI enhances these when you hit End" pill under the editor (§5.3)

We ship one base `Pill` with `tone` prop (blue / matcha / straw / neutral) + two named export variants that wrap it: `RecordingBadge` and `ProviderChip`. Future phases can compose more wrappers; this avoids the trap of one mega-component with twelve flags.

- [ ] **Step 1: Write the failing test.**

Create `web/src/components/Pill.test.tsx`:

```tsx
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { Pill, RecordingBadge, ProviderChip } from "./Pill";

describe("Pill", () => {
  it("renders children", () => {
    render(<Pill>Local-only · on</Pill>);
    expect(screen.getByText(/local-only · on/i)).toBeInTheDocument();
  });

  it("defaults to a neutral tone (line border, paper bg)", () => {
    const { container } = render(<Pill>x</Pill>);
    const el = container.firstChild as HTMLElement;
    expect(el.className).toMatch(/rounded-pill/);
    expect(el.className).toMatch(/border/);
  });

  it("matcha tone uses matcha-soft bg and matcha text", () => {
    const { container } = render(<Pill tone="matcha">Local-only · on</Pill>);
    const el = container.firstChild as HTMLElement;
    expect(el.className).toMatch(/bg-mtsoft/);
    expect(el.className).toMatch(/text-matcha/);
  });

  it("blue tone uses blueberry-soft bg and blueberry text", () => {
    const { container } = render(<Pill tone="blue">Active</Pill>);
    const el = container.firstChild as HTMLElement;
    expect(el.className).toMatch(/bg-blsoft/);
    expect(el.className).toMatch(/text-blue/);
  });

  it("straw tone uses strawberry-soft bg and strawberry text", () => {
    const { container } = render(<Pill tone="straw">Recording</Pill>);
    const el = container.firstChild as HTMLElement;
    expect(el.className).toMatch(/bg-strsoft/);
    expect(el.className).toMatch(/text-straw/);
  });
});

describe("RecordingBadge", () => {
  it("renders a pulsing dot + the timer", () => {
    const { container } = render(<RecordingBadge elapsed="12:04" />);
    const dot = container.querySelector('[data-testid="recording-dot"]');
    expect(dot).not.toBeNull();
    expect(dot!.className).toMatch(/animate-recpulse/);
    expect(screen.getByText("12:04")).toBeInTheDocument();
  });

  it("renders the timer in mono font", () => {
    render(<RecordingBadge elapsed="00:42" />);
    const timer = screen.getByText("00:42");
    expect(timer.className).toMatch(/font-mono/);
  });
});

describe("ProviderChip", () => {
  it("renders provider name", () => {
    render(<ProviderChip name="Ollama" />);
    expect(screen.getByText("Ollama")).toBeInTheDocument();
  });

  it("shows active state with blue tone", () => {
    const { container } = render(<ProviderChip name="Minimax" active />);
    const el = container.firstChild as HTMLElement;
    expect(el.className).toMatch(/bg-blsoft/);
  });

  it("shows local state with matcha tone", () => {
    const { container } = render(<ProviderChip name="whisper.cpp" local />);
    const el = container.firstChild as HTMLElement;
    expect(el.className).toMatch(/bg-mtsoft/);
  });
});
```

- [ ] **Step 2: Run — expect failure.**

Run: `pnpm --dir web test -- Pill`
Expected: cannot resolve `./Pill`.

- [ ] **Step 3: Implement `web/src/components/Pill.tsx`.**

```tsx
import { type ReactNode } from "react";

type Tone = "neutral" | "blue" | "matcha" | "straw";

interface PillProps {
  tone?: Tone;
  children: ReactNode;
  className?: string;
}

const BASE = [
  "inline-flex",
  "items-center",
  "gap-1.5",
  "rounded-pill",
  "px-2.5",
  "py-1",
  "text-[12px]",
  "font-medium",
  "leading-none",
  "border",
  "whitespace-nowrap",
].join(" ");

const TONE: Record<Tone, string> = {
  neutral: "bg-paper text-mut border-line",
  blue:    "bg-blsoft  text-blue   border-blue/20",
  matcha:  "bg-mtsoft  text-matcha border-matcha/25",
  straw:   "bg-strsoft text-straw  border-straw/25",
};

/**
 * Base Pill — a 999-radius badge used for chips, status indicators, and
 * keyboard hints throughout the app. See PRD §16.6.
 */
export function Pill({ tone = "neutral", children, className = "" }: PillProps) {
  return <span className={`${BASE} ${TONE[tone]} ${className}`.trim()}>{children}</span>;
}

/**
 * RecordingBadge — pulsing strawberry dot + JetBrains-Mono timer. Always
 * uses a white surface with a strawberry hairline border to match PRD §16.6.
 */
interface RecordingBadgeProps {
  /** Pre-formatted elapsed time, e.g. "12:04". */
  elapsed: string;
}

export function RecordingBadge({ elapsed }: RecordingBadgeProps) {
  return (
    <span
      className={[
        "inline-flex items-center gap-2 rounded-pill px-3 py-1.5",
        "bg-card text-ink border border-straw",
        "text-[12px] leading-none",
      ].join(" ")}
    >
      <span
        data-testid="recording-dot"
        className="inline-block w-2 h-2 rounded-pill bg-straw animate-recpulse"
        aria-hidden
      />
      <span className="font-mono text-[12px] tracking-tight">{elapsed}</span>
    </span>
  );
}

/**
 * ProviderChip — used in settings (§5.6) and onboarding (§5.10) to surface
 * LLM/STT providers. `active` paints blueberry, `local` paints matcha,
 * default is dashed-border neutral for "preset to clone".
 */
interface ProviderChipProps {
  name: string;
  active?: boolean;
  local?: boolean;
}

export function ProviderChip({ name, active, local }: ProviderChipProps) {
  let tone: Tone = "neutral";
  if (active) tone = "blue";
  else if (local) tone = "matcha";

  return (
    <Pill tone={tone} className="px-3 py-1.5 text-[12px]">
      <span
        aria-hidden
        className={[
          "inline-block w-1.5 h-1.5 rounded-pill",
          active ? "bg-blue" : local ? "bg-matcha" : "bg-mut",
        ].join(" ")}
      />
      {name}
    </Pill>
  );
}
```

- [ ] **Step 4: Run — expect PASS.**

Run: `pnpm --dir web test -- Pill`
Expected: `9 passed`.

- [ ] **Step 5: Commit.**

```bash
git add web/src/components/Pill.tsx web/src/components/Pill.test.tsx
git commit -m "feat(web): add Pill component with RecordingBadge + ProviderChip variants"
```

---

### Task 1.6 · `Card` component

**Files:**
- Create: `web/src/components/Card.tsx`
- Create: `web/src/components/Card.test.tsx`

The `Card` is the white surface with `--shadow-card` and `--radius-card` (14px) used everywhere meetings, settings, and onboarding render content (§5.6 settings cards, §5.9 meeting cards, §5.10 onboarding step cards). A few use a blueberry hairline border to denote active/current state.

- [ ] **Step 1: Write the failing test.**

Create `web/src/components/Card.test.tsx`:

```tsx
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { Card } from "./Card";

describe("Card", () => {
  it("renders children", () => {
    render(
      <Card>
        <h2>Hello</h2>
      </Card>
    );
    expect(screen.getByRole("heading", { name: /hello/i })).toBeInTheDocument();
  });

  it("applies card surface classes by default", () => {
    const { container } = render(<Card>x</Card>);
    const el = container.firstChild as HTMLElement;
    expect(el.className).toMatch(/bg-card/);
    expect(el.className).toMatch(/rounded-card/);
    expect(el.className).toMatch(/shadow-card/);
  });

  it("supports an 'active' variant with a blueberry hairline border", () => {
    const { container } = render(<Card active>active</Card>);
    const el = container.firstChild as HTMLElement;
    expect(el.className).toMatch(/border-blue/);
  });

  it("supports padding sizes", () => {
    const { container } = render(<Card padding="lg">x</Card>);
    const el = container.firstChild as HTMLElement;
    expect(el.className).toMatch(/p-8/);
  });

  it("renders as an <article> when as='article'", () => {
    const { container } = render(<Card as="article">x</Card>);
    expect(container.querySelector("article")).not.toBeNull();
  });
});
```

- [ ] **Step 2: Run — expect failure.**

Run: `pnpm --dir web test -- Card`
Expected: cannot resolve `./Card`.

- [ ] **Step 3: Implement `web/src/components/Card.tsx`.**

```tsx
import { type ReactNode, type ElementType } from "react";

type Padding = "sm" | "md" | "lg";

interface CardProps {
  children: ReactNode;
  /** Wraps the card in a 1.5px blueberry border for "current step" / active states. */
  active?: boolean;
  /** Padding scale (4-base): sm=12, md=20, lg=32. */
  padding?: Padding;
  /** Element to render as. Defaults to <div>. Use 'article' for meeting cards. */
  as?: ElementType;
  className?: string;
}

const PADDING: Record<Padding, string> = {
  sm: "p-3",
  md: "p-5",
  lg: "p-8",
};

/**
 * Card — white 14px-radius surface with the standard card elevation.
 * Composes the §16.4 elevation token and §16.4 radius token.
 *
 * Use `active` for the "current step" visual in onboarding (§5.10) and the
 * active-provider card in settings (§5.6). Use `padding="lg"` for hero
 * onboarding cards, "md" for settings rows, "sm" for compact meeting cards.
 */
export function Card({
  children,
  active = false,
  padding = "md",
  as,
  className = "",
}: CardProps) {
  const Tag = (as ?? "div") as ElementType;
  const border = active ? "border-[1.5px] border-blue" : "border border-line";
  const cls = [
    "bg-card",
    "rounded-card",
    "shadow-card",
    border,
    PADDING[padding],
    className,
  ]
    .join(" ")
    .trim();

  return <Tag className={cls}>{children}</Tag>;
}
```

- [ ] **Step 4: Run — expect PASS.**

Run: `pnpm --dir web test -- Card`
Expected: `5 passed`.

- [ ] **Step 5: Commit.**

```bash
git add web/src/components/Card.tsx web/src/components/Card.test.tsx
git commit -m "feat(web): add Card component with active variant and padding scale"
```

---

### Task 1.7 · `BrowserChrome` component (fake-Safari wrapper for mockups)

**Files:**
- Create: `web/src/components/BrowserChrome.tsx`
- Create: `web/src/components/BrowserChrome.test.tsx`

Per PRD §16.6: "every full-screen mock uses a fake-Safari header (`42px` height, `#F4EEE3` bg, 3-color traffic-light dots, centered URL pill showing `localhost:7878/...`)". Used in marketing screenshots, onboarding mock previews, and the style-guide mockups section.

- [ ] **Step 1: Write the failing test.**

Create `web/src/components/BrowserChrome.test.tsx`:

```tsx
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { BrowserChrome } from "./BrowserChrome";

describe("BrowserChrome", () => {
  it("renders the URL in the centered pill", () => {
    render(
      <BrowserChrome url="localhost:7878/welcome">
        <div>inner</div>
      </BrowserChrome>
    );
    expect(screen.getByText("localhost:7878/welcome")).toBeInTheDocument();
  });

  it("renders three traffic-light dots", () => {
    const { container } = render(
      <BrowserChrome url="x">
        <div />
      </BrowserChrome>
    );
    const dots = container.querySelectorAll('[data-testid="traffic-dot"]');
    expect(dots.length).toBe(3);
  });

  it("renders children inside the chrome body", () => {
    render(
      <BrowserChrome url="x">
        <p data-testid="content">hello</p>
      </BrowserChrome>
    );
    expect(screen.getByTestId("content")).toBeInTheDocument();
  });

  it("uses the window-shadow elevation", () => {
    const { container } = render(
      <BrowserChrome url="x">
        <div />
      </BrowserChrome>
    );
    const root = container.firstChild as HTMLElement;
    expect(root.className).toMatch(/shadow-/);
  });
});
```

- [ ] **Step 2: Run — expect failure.**

Run: `pnpm --dir web test -- BrowserChrome`
Expected: cannot resolve `./BrowserChrome`.

- [ ] **Step 3: Implement `web/src/components/BrowserChrome.tsx`.**

```tsx
import { type ReactNode } from "react";

interface BrowserChromeProps {
  /** URL to show in the centered pill, e.g. "localhost:7878/welcome". */
  url: string;
  children: ReactNode;
  /** Optional className for outer sizing — typically rounded corners + width. */
  className?: string;
}

/**
 * BrowserChrome — fake-Safari window for full-screen mockups (PRD §16.6).
 * Used by marketing screenshots, the onboarding "boot sequence" preview,
 * and the style-guide mockup gallery.
 *
 * Layout: 42px header on a paper-warm bg (#F4EEE3) with three traffic-light
 * dots on the left and a centered URL pill. Body is whatever children render.
 */
export function BrowserChrome({ url, children, className = "" }: BrowserChromeProps) {
  return (
    <div
      className={[
        "overflow-hidden rounded-card border border-line bg-card",
        "shadow-[0_26px_60px_-28px_rgba(40,30,15,0.4)]",
        className,
      ]
        .join(" ")
        .trim()}
    >
      {/* Header */}
      <div
        className="h-[42px] flex items-center px-4 border-b border-line"
        style={{ background: "#F4EEE3" }}
      >
        {/* Traffic-light dots */}
        <div className="flex items-center gap-2">
          <span
            data-testid="traffic-dot"
            className="inline-block w-3 h-3 rounded-pill"
            style={{ background: "#FF5F57" }}
            aria-hidden
          />
          <span
            data-testid="traffic-dot"
            className="inline-block w-3 h-3 rounded-pill"
            style={{ background: "#FEBC2E" }}
            aria-hidden
          />
          <span
            data-testid="traffic-dot"
            className="inline-block w-3 h-3 rounded-pill"
            style={{ background: "#28C840" }}
            aria-hidden
          />
        </div>

        {/* Centered URL pill */}
        <div className="flex-1 flex justify-center">
          <span
            className={[
              "inline-block rounded-pill px-3 py-1",
              "bg-card border border-line",
              "font-mono text-[11px] text-mut",
            ].join(" ")}
          >
            {url}
          </span>
        </div>

        {/* Right-side spacer to keep URL truly centered (mirrors dot width) */}
        <div className="w-[52px]" aria-hidden />
      </div>

      {/* Body */}
      <div className="bg-paper">{children}</div>
    </div>
  );
}
```

- [ ] **Step 4: Run — expect PASS.**

Run: `pnpm --dir web test -- BrowserChrome`
Expected: `4 passed`.

- [ ] **Step 5: Commit.**

```bash
git add web/src/components/BrowserChrome.tsx web/src/components/BrowserChrome.test.tsx
git commit -m "feat(web): add BrowserChrome mockup wrapper (fake-Safari header)"
```

---

### Task 1.8 · React Router 7 setup with `/` and `/style-guide` routes

**Files:**
- Modify: `web/package.json` (add `react-router`)
- Create: `web/src/router.tsx`
- Create: `web/src/routes/StyleGuide.tsx` (placeholder; full content in Task 1.9)
- Modify: `web/src/main.tsx` (mount `<RouterProvider>`)
- Modify: `web/src/App.tsx` (link to /style-guide)
- Modify: `web/src/App.test.tsx` (re-anchor selectors)

React Router 7 renamed the package from `react-router-dom` to `react-router`. The `createBrowserRouter` + `RouterProvider` pattern is the v6.4+ recommended approach.

- [ ] **Step 1: Install react-router.**

Run: `pnpm --dir web add react-router@^7`
Expected: `react-router` added under `dependencies` in `web/package.json`.

- [ ] **Step 2: Write the failing router test.**

We need a tiny seam to verify the route map is wired. Create `web/src/router.test.tsx`:

```tsx
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { RouterProvider, createMemoryRouter } from "react-router";
import { routes } from "./router";

describe("router", () => {
  it("renders the App at /", async () => {
    const router = createMemoryRouter(routes, { initialEntries: ["/"] });
    render(<RouterProvider router={router} />);
    expect(await screen.findByRole("heading", { name: /yogurt/i })).toBeInTheDocument();
  });

  it("renders the StyleGuide at /style-guide", async () => {
    const router = createMemoryRouter(routes, { initialEntries: ["/style-guide"] });
    render(<RouterProvider router={router} />);
    expect(await screen.findByRole("heading", { name: /style guide/i })).toBeInTheDocument();
  });
});
```

(Note: `App` will mock `fetchHealth` from `./lib/api` here via the existing Phase 0 setup if you keep the App test's `vi.mock` global — but `vi.mock` in another file does not leak across files. Since this router test renders `App`, which calls `fetchHealth` on mount in a useEffect, we should mock it locally too. Add at the top of `router.test.tsx`:)

```tsx
import { vi } from "vitest";
vi.mock("./lib/api", () => ({
  fetchHealth: vi.fn().mockResolvedValue({ status: "ok", service: "yogurt-server" }),
}));
```

- [ ] **Step 3: Run — expect failure.**

Run: `pnpm --dir web test -- router`
Expected: cannot resolve `./router` (or `./routes/StyleGuide`).

- [ ] **Step 4: Create the placeholder `StyleGuide` route.**

Create `web/src/routes/StyleGuide.tsx`:

```tsx
/**
 * Style guide — renders every design token and component primitive. Serves
 * as living documentation of the system and the human-eyeball check that
 * tokens are wired correctly. Filled in fully in Task 1.9.
 */
export function StyleGuide() {
  return (
    <main className="mx-auto max-w-5xl px-8 py-12">
      <h1 className="font-serif text-5xl text-ink">Style guide</h1>
      <p className="mt-2 text-mut">Coming together in Task 1.9.</p>
    </main>
  );
}
```

- [ ] **Step 5: Create `web/src/router.tsx`.**

```tsx
import { createBrowserRouter, type RouteObject } from "react-router";
import { App } from "./App";
import { StyleGuide } from "./routes/StyleGuide";

/**
 * Top-level route table. Phase 1 ships two routes:
 *   "/"            — the Phase-0 hello-world App (TipTap demo + health line)
 *   "/style-guide" — the design-system showcase (Task 1.9)
 *
 * Future phases will replace "/" with the library home (§5.9) and add
 * "/settings", "/welcome", "/meetings/:id", etc.
 */
export const routes: RouteObject[] = [
  { path: "/", element: <App /> },
  { path: "/style-guide", element: <StyleGuide /> },
];

export const router = createBrowserRouter(routes);
```

- [ ] **Step 6: Modify `web/src/main.tsx` to mount the router.**

Replace the existing render call. Keep the font imports + index.css.

```tsx
// Font registration — side-effect imports (unchanged from Task 1.2).
import "@fontsource/instrument-serif/400.css";
import "@fontsource/instrument-serif/400-italic.css";
import "@fontsource/hanken-grotesk/400.css";
import "@fontsource/hanken-grotesk/500.css";
import "@fontsource/hanken-grotesk/600.css";
import "@fontsource/hanken-grotesk/700.css";
import "@fontsource/hanken-grotesk/800.css";
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/500.css";
import "@fontsource/jetbrains-mono/600.css";

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { RouterProvider } from "react-router";
import { router } from "./router";
import "./index.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <RouterProvider router={router} />
  </StrictMode>
);
```

- [ ] **Step 7: Modify `web/src/App.tsx` to use the new components and link to /style-guide.**

Replace the existing `App.tsx` body. Keep the TipTap + health imports.

```tsx
import { useEditor, EditorContent } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { useEffect, useState } from "react";
import { Link } from "react-router";
import { fetchHealth, type HealthResponse } from "./lib/api";
import { Logo } from "./components/Logo";
import { Card } from "./components/Card";
import { Pill } from "./components/Pill";

export function App() {
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const editor = useEditor({
    extensions: [StarterKit],
    content: "<p>Type something — TipTap is working.</p>",
  });

  useEffect(() => {
    fetchHealth().then(setHealth).catch((e) => console.error(e));
  }, []);

  return (
    <main className="mx-auto max-w-2xl px-10 py-12 space-y-6">
      <header className="flex items-center gap-3">
        <Logo size={44} ariaLabel="Yogurt" />
        <div>
          <h1 className="font-serif text-[44px] leading-none text-ink">yogurt</h1>
          <p className="mt-1 text-[13px] text-mut">
            phase 1 design system ·{" "}
            <Link to="/style-guide" className="text-blue underline-offset-2 hover:underline">
              /style-guide →
            </Link>
          </p>
        </div>
      </header>

      <div>
        <Pill tone="matcha">
          server:{" "}
          <code className="font-mono">
            {health ? `${health.service} ${health.status}` : "loading…"}
          </code>
        </Pill>
      </div>

      <Card padding="md">
        <EditorContent editor={editor} />
      </Card>
    </main>
  );
}
```

- [ ] **Step 8: Update `web/src/App.test.tsx` for the new markup.**

The Phase-0 test asserted `text-neutral-500` markup that's now gone. Replace with assertions against the new structure. Open `web/src/App.test.tsx` and replace its body with:

```tsx
import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { App } from "./App";

vi.mock("./lib/api", () => ({
  fetchHealth: vi.fn().mockResolvedValue({ status: "ok", service: "yogurt-server" }),
}));

function renderApp() {
  return render(
    <MemoryRouter>
      <App />
    </MemoryRouter>
  );
}

describe("App", () => {
  it("renders the yogurt headline", async () => {
    renderApp();
    expect(await screen.findByRole("heading", { name: /yogurt/i })).toBeInTheDocument();
  });

  it("shows the health response once fetched", async () => {
    renderApp();
    await waitFor(() => {
      expect(screen.getByText(/yogurt-server ok/)).toBeInTheDocument();
    });
  });

  it("links to the style guide", async () => {
    renderApp();
    const link = await screen.findByRole("link", { name: /style-guide/i });
    expect(link).toHaveAttribute("href", "/style-guide");
  });
});
```

(`<App />` now uses `<Link>` from `react-router`, which requires a router context. Wrapping in `<MemoryRouter>` is the standard `@testing-library/react` pattern for v7.)

- [ ] **Step 9: Run all tests — expect PASS.**

Run: `pnpm --dir web test`
Expected: every test file passes. Specifically:
- `Logo.test.tsx`: 4 passed
- `Button.test.tsx`: 7 passed
- `Pill.test.tsx`: 9 passed
- `Card.test.tsx`: 5 passed
- `BrowserChrome.test.tsx`: 4 passed
- `router.test.tsx`: 2 passed
- `App.test.tsx`: 3 passed

Total: 34 passed.

- [ ] **Step 10: Manual smoke.**

Run: `pnpm --dir web dev`
Open `http://localhost:5173/`. Expected: Logo + "yogurt" headline in Instrument Serif, matcha pill with server status, TipTap editor inside a Card.
Click the "/style-guide →" link. Expected: navigates to `/style-guide`, shows the placeholder "Style guide" heading in Instrument Serif.

Stop Vite.

- [ ] **Step 11: Commit.**

```bash
git add web/package.json web/pnpm-lock.yaml web/src/router.tsx web/src/router.test.tsx \
        web/src/routes/StyleGuide.tsx web/src/main.tsx web/src/App.tsx web/src/App.test.tsx
git commit -m "feat(web): add react-router 7 with / and /style-guide routes, wire components into App"
```

---

### Task 1.9 · `/style-guide` route — visual proof everything is wired

**Files:**
- Modify: `web/src/routes/StyleGuide.tsx` (expand from placeholder)

The style guide is the human-validation surface. It renders every token (color swatches, font specimens, radius examples, shadow examples, motion previews) and every component primitive (Logo at multiple sizes, all Button variants, every Pill tone, RecordingBadge, ProviderChip variants, Card with and without `active`, BrowserChrome wrapping a small mock). It's intentionally a single long scrollable page — no fancy navigation, no tabs. Just sections separated by `<hr>`.

No new tests for this task — the components it composes are already tested. Manual eyeball check is the verification.

- [ ] **Step 1: Replace `web/src/routes/StyleGuide.tsx` with the full showcase.**

```tsx
import { Logo } from "../components/Logo";
import { Button } from "../components/Button";
import { Pill, RecordingBadge, ProviderChip } from "../components/Pill";
import { Card } from "../components/Card";
import { BrowserChrome } from "../components/BrowserChrome";

const COLORS: Array<{ token: string; hex: string; use: string }> = [
  { token: "paper",   hex: "#FBF7EF", use: "App background, hero surfaces" },
  { token: "card",    hex: "#FFFFFF", use: "Cards, surfaces over paper" },
  { token: "ink",     hex: "#211D18", use: "User notes, headings, primary text" },
  { token: "grey",    hex: "#A89F90", use: "AI-added text, secondary captions" },
  { token: "line",    hex: "#EBE3D5", use: "Borders, dividers" },
  { token: "blue",    hex: "#5B4FC7", use: "Primary blueberry — buttons, transcript links" },
  { token: "blsoft",  hex: "#ECE9FB", use: "Soft blueberry — active-nav, pill bg" },
  { token: "straw",   hex: "#E07A66", use: "Recording indicator, error/warning accent" },
  { token: "strsoft", hex: "#FBE6E0", use: "Soft strawberry — error/warning bg" },
  { token: "matcha",  hex: "#5E9E73", use: "Local-only / privacy / success" },
  { token: "mtsoft",  hex: "#E7F0E8", use: "Soft matcha — local-mode badges" },
  { token: "mut",     hex: "#8A8174", use: "Muted text on cards" },
];

const RADII = [
  { name: "chip",   px: 6,  bg: "bg-blsoft" },
  { name: "button", px: 9,  bg: "bg-blsoft" },
  { name: "card",   px: 14, bg: "bg-card border border-line" },
  { name: "pill",   px: 999, bg: "bg-mtsoft" },
];

const SHADOWS = [
  { name: "shadow-card",   className: "shadow-card",  use: "Card surface (default)" },
  { name: "shadow-pop",    className: "shadow-[0_12px_30px_-10px_rgba(40,30,15,0.22)]", use: "Chat window, popovers" },
  { name: "shadow-window", className: "shadow-[0_26px_60px_-28px_rgba(40,30,15,0.4)]",  use: "Modals, mockup chrome" },
];

const MOTION = [
  { name: "recpulse",        duration: "1.4s",  use: "Recording / enhancing dot pulse", className: "animate-recpulse w-3 h-3 rounded-pill bg-straw" },
  { name: "blink",           duration: "1.0s",  use: "Cursor blink for active editor", className: "animate-blink w-[2px] h-5 bg-ink" },
  { name: "shimmer",         duration: "1.25s", use: "Skeleton placeholders during enhance", className: "animate-shimmer w-32 h-3 rounded-chip bg-gradient-to-r from-line via-paper to-line bg-[length:400%_100%]" },
  { name: "wave",            duration: "1.0s",  use: "3-bar audio wave on transcript tab", className: "animate-wave inline-block w-1 h-4 bg-blue rounded-pill origin-center" },
  { name: "float",           duration: "3.5s",  use: "Empty-state logo gentle float", className: "animate-float inline-block" },
  { name: "pop-up",          duration: "260ms", use: "Chat window expanding from Ask pill", className: "" },
  { name: "slide-in-right",  duration: "340ms", use: "Live transcript dock opening", className: "" },
];

export function StyleGuide() {
  return (
    <main className="mx-auto max-w-5xl px-8 py-12 space-y-16">
      {/* Header */}
      <header className="flex items-center gap-4">
        <Logo size={60} ariaLabel="Yogurt" />
        <div>
          <h1 className="font-serif text-[52px] leading-none tracking-tight text-ink">
            Yogurt style guide
          </h1>
          <p className="mt-2 text-[14px] text-mut max-w-xl">
            Every token and component primitive defined in PRD §16, rendered as
            living documentation. If anything below looks wrong, the design
            system is wrong — fix it here.
          </p>
        </div>
      </header>

      {/* ─── Color tokens ─────────────────────────────────────────────────── */}
      <Section title="Color tokens (PRD §16.2)" caption="Blueberry theme — the only v1 theme.">
        <div className="grid grid-cols-2 gap-4 md:grid-cols-3">
          {COLORS.map((c) => (
            <Card key={c.token} padding="sm">
              <div
                className="h-16 w-full rounded-chip border border-line"
                style={{ background: c.hex }}
                aria-label={`Swatch for --color-${c.token}`}
              />
              <div className="mt-3">
                <div className="font-mono text-[11px] text-mut">--color-{c.token}</div>
                <div className="font-mono text-[12px] text-ink">{c.hex}</div>
                <div className="mt-1 text-[12px] text-mut leading-snug">{c.use}</div>
              </div>
            </Card>
          ))}
        </div>
      </Section>

      {/* ─── Typography ───────────────────────────────────────────────────── */}
      <Section title="Typography (PRD §16.3)" caption="Three families. Use them deliberately.">
        <Card padding="lg">
          <div className="space-y-6">
            <div>
              <p className="font-mono text-[11px] text-mut uppercase tracking-wide">
                Instrument Serif · 52 / 38 / 30
              </p>
              <p className="font-serif text-[52px] leading-none text-ink mt-1">yogurt</p>
              <p className="font-serif text-[38px] leading-tight text-ink">Welcome to yogurt.</p>
              <p className="font-serif text-[30px] leading-tight text-ink">Good afternoon, Dana</p>
              <p className="font-serif italic text-[20px] text-mut">
                A local-first, open-source meeting copilot.
              </p>
            </div>
            <div>
              <p className="font-mono text-[11px] text-mut uppercase tracking-wide">
                Hanken Grotesk · 400 / 500 / 600 / 700 / 800
              </p>
              <p className="font-sans font-normal text-[15px] text-ink">
                The quick brown fox jumps over the lazy dog. 400.
              </p>
              <p className="font-sans font-medium text-[15px] text-ink">
                The quick brown fox jumps over the lazy dog. 500.
              </p>
              <p className="font-sans font-semibold text-[15px] text-ink">
                The quick brown fox jumps over the lazy dog. 600.
              </p>
              <p className="font-sans font-bold text-[16px] text-ink">
                Card title style — 16px Hanken 700.
              </p>
              <p className="font-sans font-extrabold text-[15px] text-ink">
                The quick brown fox jumps over the lazy dog. 800.
              </p>
            </div>
            <div>
              <p className="font-mono text-[11px] text-mut uppercase tracking-wide">
                JetBrains Mono · 400 / 500 / 600
              </p>
              <p className="font-mono text-[12px] text-ink">$ yogurt start</p>
              <p className="font-mono text-[12px] text-mut">
                ✓ server live on :7878
              </p>
              <p className="font-mono text-[11px] text-blue">localhost:7878/welcome</p>
              <p className="font-mono text-[12px] text-ink">00:11:02</p>
            </div>
          </div>
        </Card>
      </Section>

      {/* ─── Spacing ──────────────────────────────────────────────────────── */}
      <Section title="Spacing (PRD §16.4)" caption="4-base scale. Nothing off-scale.">
        <Card padding="md">
          <div className="flex items-end gap-4">
            {[4, 8, 12, 16, 24, 32, 48].map((px) => (
              <div key={px} className="flex flex-col items-center gap-2">
                <div
                  className="bg-blue rounded-chip"
                  style={{ width: px, height: px }}
                  aria-label={`${px}px spacing block`}
                />
                <span className="font-mono text-[11px] text-mut">{px}</span>
              </div>
            ))}
          </div>
        </Card>
      </Section>

      {/* ─── Border radius ────────────────────────────────────────────────── */}
      <Section title="Border radius (PRD §16.4)" caption="Four scales — pick the right one.">
        <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
          {RADII.map((r) => (
            <Card key={r.name} padding="sm">
              <div
                className={`h-20 w-full ${r.bg}`}
                style={{ borderRadius: r.px }}
                aria-label={`${r.name} radius example`}
              />
              <div className="mt-3">
                <div className="font-mono text-[11px] text-mut">rounded-{r.name}</div>
                <div className="font-mono text-[12px] text-ink">{r.px}px</div>
              </div>
            </Card>
          ))}
        </div>
      </Section>

      {/* ─── Elevation ────────────────────────────────────────────────────── */}
      <Section title="Elevation (PRD §16.4)" caption="Three depths. Use sparingly.">
        <div className="grid gap-6 md:grid-cols-3">
          {SHADOWS.map((s) => (
            <div
              key={s.name}
              className={`bg-card rounded-card border border-line p-6 ${s.className}`}
            >
              <div className="font-mono text-[11px] text-mut">{s.name}</div>
              <div className="mt-1 text-[13px] text-ink">{s.use}</div>
            </div>
          ))}
        </div>
      </Section>

      {/* ─── Motion ───────────────────────────────────────────────────────── */}
      <Section title="Motion (PRD §16.5)" caption="Each animation, live. Don't add new ones.">
        <Card padding="md">
          <div className="space-y-4">
            {MOTION.map((m) => (
              <div key={m.name} className="flex items-center gap-4">
                <div className="w-40">
                  <div className="font-mono text-[11px] text-mut">{m.name}</div>
                  <div className="font-mono text-[12px] text-ink">{m.duration}</div>
                </div>
                <div className="w-40 h-8 flex items-center">
                  {m.className ? <span className={m.className} aria-hidden /> : <span className="text-[11px] text-mut italic">scripted — see chat / transcript</span>}
                </div>
                <div className="flex-1 text-[12px] text-mut">{m.use}</div>
              </div>
            ))}
          </div>
        </Card>
      </Section>

      {/* ─── Logo at multiple sizes ──────────────────────────────────────── */}
      <Section title="Logo (PRD §16.1)" caption="Renders cleanly from 19px to 60px.">
        <Card padding="lg">
          <div className="flex items-end gap-8">
            {[19, 24, 32, 44, 60].map((size) => (
              <div key={size} className="flex flex-col items-center gap-2">
                <Logo size={size} ariaLabel={`Yogurt ${size}px`} />
                <span className="font-mono text-[11px] text-mut">{size}px</span>
              </div>
            ))}
          </div>
        </Card>
      </Section>

      {/* ─── Buttons ──────────────────────────────────────────────────────── */}
      <Section title="Buttons (PRD §16.6)" caption="Three variants. Primary is the action; secondary is the alternative; ghost is the dismissal.">
        <Card padding="lg">
          <div className="space-y-6">
            <div className="flex items-center gap-4 flex-wrap">
              <Button>+ New meeting</Button>
              <Button>Take me to my meetings →</Button>
              <Button>Re-enhance ⌄</Button>
              <Button disabled>Disabled primary</Button>
            </div>
            <div className="flex items-center gap-4 flex-wrap">
              <Button variant="secondary">Enhance</Button>
              <Button variant="secondary">Restart Yogurt</Button>
              <Button variant="secondary">Cancel</Button>
              <Button variant="secondary" disabled>Disabled secondary</Button>
            </div>
            <div className="flex items-center gap-4 flex-wrap">
              <Button variant="ghost">Cancel</Button>
              <Button variant="ghost">Dismiss</Button>
              <Button variant="ghost" disabled>Disabled ghost</Button>
            </div>
          </div>
        </Card>
      </Section>

      {/* ─── Pills ────────────────────────────────────────────────────────── */}
      <Section title="Pills, badges, chips (PRD §16.6)" caption="Pill is the base; RecordingBadge and ProviderChip are named wrappers.">
        <Card padding="lg">
          <div className="space-y-6">
            <div className="flex items-center gap-3 flex-wrap">
              <Pill>neutral</Pill>
              <Pill tone="blue">Active</Pill>
              <Pill tone="matcha">Local-only · on</Pill>
              <Pill tone="straw">Recording</Pill>
            </div>
            <div className="flex items-center gap-3 flex-wrap">
              <RecordingBadge elapsed="00:42" />
              <RecordingBadge elapsed="12:04" />
              <RecordingBadge elapsed="38:11" />
            </div>
            <div className="flex items-center gap-3 flex-wrap">
              <ProviderChip name="Minimax" active />
              <ProviderChip name="OpenAI" />
              <ProviderChip name="OpenRouter" />
              <ProviderChip name="Ollama" local />
              <ProviderChip name="whisper.cpp" local />
            </div>
          </div>
        </Card>
      </Section>

      {/* ─── Cards ────────────────────────────────────────────────────────── */}
      <Section title="Cards" caption="Three paddings; optional active border.">
        <div className="grid gap-4 md:grid-cols-3">
          <Card padding="sm">
            <h3 className="font-sans font-bold text-[16px] text-ink">Small card</h3>
            <p className="mt-1 text-[12px] text-mut">padding="sm" — compact rows.</p>
          </Card>
          <Card padding="md">
            <h3 className="font-sans font-bold text-[16px] text-ink">Medium card</h3>
            <p className="mt-1 text-[12px] text-mut">padding="md" (default) — most surfaces.</p>
          </Card>
          <Card padding="lg" active>
            <h3 className="font-sans font-bold text-[16px] text-ink">Active card</h3>
            <p className="mt-1 text-[12px] text-mut">padding="lg" + active — current step in onboarding.</p>
          </Card>
        </div>
      </Section>

      {/* ─── BrowserChrome ───────────────────────────────────────────────── */}
      <Section title="BrowserChrome" caption="Fake-Safari wrapper for marketing mockups.">
        <BrowserChrome url="localhost:7878/welcome">
          <div className="p-10 flex items-center gap-4">
            <Logo size={32} />
            <div>
              <h3 className="font-serif text-[28px] text-ink leading-none">Welcome to yogurt.</h3>
              <p className="mt-2 text-[13px] text-mut">
                Two streams, one set of notes, zero bots in the call.
              </p>
            </div>
          </div>
        </BrowserChrome>
      </Section>

      {/* ─── Footer ──────────────────────────────────────────────────────── */}
      <footer className="border-t border-line pt-6">
        <p className="font-mono text-[11px] text-mut">
          PRD §16 · Phase 1 · last updated{" "}
          <span className="text-ink">2026-06-25</span>
        </p>
      </footer>
    </main>
  );
}

function Section({
  title,
  caption,
  children,
}: {
  title: string;
  caption: string;
  children: React.ReactNode;
}) {
  return (
    <section>
      <h2 className="font-serif text-[30px] text-ink leading-tight">{title}</h2>
      <p className="mt-1 text-[13px] text-mut">{caption}</p>
      <div className="mt-5">{children}</div>
    </section>
  );
}
```

- [ ] **Step 2: Run existing tests — expect PASS unchanged.**

Run: `pnpm --dir web test`
Expected: same 34 tests passing as Task 1.8. The expanded `StyleGuide` doesn't need its own tests — every primitive it composes is already covered.

- [ ] **Step 3: Manual visual check.**

Run: `pnpm --dir web dev`
Open `http://localhost:5173/style-guide`.

Verify with your eyes:
- All 12 color swatches render with correct hex values.
- Type specimens visibly render in the three different families (serif headlines distinct from sans body distinct from mono captions).
- Spacing blocks form a visibly proportional 4-base ladder.
- Radius examples are visually distinguishable (`chip` < `button` < `card`, `pill` is a circle/oval).
- Three shadow cards have visibly different elevation strengths.
- Motion section: `recpulse` dot pulses ~once/1.4s, `blink` blinks ~once/1s, `shimmer` rectangle has a visible sliding gradient, `wave` bar squashes/expands, `float` logo gently rises and falls.
- All three Button variants render distinct (blueberry / cream-bordered / transparent).
- Disabled buttons appear washed out and don't trigger click animations on hover.
- Recording badges show a pulsing strawberry dot + mono timer.
- Provider chips: Minimax has blueberry-soft bg, Ollama / whisper.cpp have matcha-soft bg.
- Cards in all three paddings render visibly different spacing.
- Active card has a noticeable blueberry hairline.
- BrowserChrome shows three traffic-light dots + centered URL pill + the wrapped logo content.

Stop Vite.

- [ ] **Step 4: Commit.**

```bash
git add web/src/routes/StyleGuide.tsx
git commit -m "feat(web): expand /style-guide with full token + component showcase"
```

---

### Task 1.10 · End-to-end smoke + acceptance check

**Files:** none — verification only.

- [ ] **Step 1: Run the full test suite.**

Run: `pnpm --dir web test`
Expected: all 34 tests pass, 0 failures.

Run: `cargo test --workspace`
Expected: all Phase-0 tests still pass — Phase 1 did not touch any Rust code.

- [ ] **Step 2: Production build.**

Run: `pnpm --dir web build`
Expected: `tsc` succeeds (no type errors), `vite build` succeeds. Output:
- `web/dist/index.html` exists.
- `web/dist/assets/*.css` includes the @theme variable declarations.
- `web/dist/assets/*.woff2` — at least 10 font files (the imported weights).
- `web/dist/assets/*.js` chunk includes the Router + Logo + Button + Pill + Card + BrowserChrome + StyleGuide code.

`du -sh web/dist` should still be under ~1.5 MB (fonts dominate; Tailwind 4 only ships used utilities).

- [ ] **Step 3: Dev-mode visual smoke (both terminals).**

Terminal 1: `pnpm --dir web dev`
Terminal 2: `cargo run -p yogurt -- start --dev --no-open`

Open `http://localhost:7878/` — should see the Phase-1-styled hello page (Logo, Hanken headline, matcha health pill, TipTap inside a Card).
Open `http://localhost:7878/style-guide` — should see the full style guide page, all motion animations live.

Stop both processes.

- [ ] **Step 4: Release-mode smoke (single binary).**

```bash
pnpm --dir web build
cargo build --release
./target/release/yogurt start --no-open &
sleep 1
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:7878/
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:7878/style-guide
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:7878/api/health
kill %1
```

Expected: three `200` responses. The `/style-guide` returns 200 because of the SPA fallback in `assets.rs` (Task 0.7) — the server returns `index.html`, and the client-side router handles the route.

- [ ] **Step 5: Lint and format.**

Run: `cargo fmt --all && cargo clippy --all-targets -- -D warnings`
Expected: clean. (No Rust files were touched but Phase-0 hygiene must still hold.)

- [ ] **Step 6: Commit any stray formatting fixes (if any) and push.**

If `cargo fmt` made changes, commit them as `chore: fmt`. Otherwise skip.

```bash
git push origin main
```

- [ ] **Step 7: Verify on GitHub.**

Open the repo in browser. Confirm the new files in `web/src/components/` and `web/src/routes/StyleGuide.tsx` appear. README quickstart from Phase 0 should still work end-to-end on a fresh clone.

- [ ] **Step 8: (Optional) tag phase milestone — only with explicit user confirmation.**

```bash
git tag -a v0.0.2-phase-1 -m "Phase 1 complete: design system + style guide"
git push origin v0.0.2-phase-1
```

---

## Phase 1 acceptance criteria

All five must be true:

1. **Tests pass:** `pnpm --dir web test` shows 34/34 green, `cargo test --workspace` still passes unchanged from Phase 0.
2. **Build succeeds:** `pnpm --dir web build` produces a deployable `web/dist` with embedded fonts.
3. **Tokens applied:** the body background is `#FBF7EF`, ink text is `#211D18`, and the matcha pill on `/` renders with `#E7F0E8` bg + `#5E9E73` text. (Inspect via DevTools → Computed.)
4. **Fonts loaded:** DevTools → Network → "font" filter shows ~10 woff2 requests, all 200; no `FontFace failed` warnings in console; the wordmark on `/` renders in Instrument Serif (visibly distinct from system fallback).
5. **Style guide visual check:** `/style-guide` renders every section enumerated in Task 1.9 Step 3 without overflow, without console errors, with all five motion animations visibly running.

## What this phase does NOT do

Explicitly out of scope (next plans cover these):
- The library home view with sidebar + meeting list (Phase 7)
- The settings page with provider cards (Phase 5)
- The onboarding 3-step welcome flow (Phase 7)
- The empty / permission-denied / enhancing / model-download states (Phase 7; enhancing motion in Phase 4)
- TipTap custom marks for `aiGrey` + transcript deep-links (Phase 4)
- Tab group component (Phase 4)
- Real icon system selection (Phase 7 — Lucide vs Phosphor vs custom)
- Strawberry / Matcha-dark themes (deferred per PRD §15 — Blueberry only in v1)
- Live `recpulse` dot tied to actual recording state (Phase 2 wires it; Phase 1 only verifies the animation works)

## Next plan

After Phase 1 lands, write `docs/superpowers/plans/<date>-yogurt-phase-2-audio-capture.md` covering:
- `yogurt-audio` crate skeleton (Cargo.toml, trait surface)
- ScreenCaptureKit init via the `screencapturekit` Rust crate + first-run macOS Screen Recording permission flow
- Default-input-device mic capture
- Both streams pushed onto a `tokio::sync::broadcast` channel as `(channel, [i16])` frames
- The "Permission not granted" recovery screen (composes Card + Button + Pill from Phase 1)
- Integration test: spawn capture for 2s in CI, assert N frames arrived on each channel

Subsequent phase plans follow the PRD §12 roadmap.
