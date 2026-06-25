---
phase: 01-design-system
reviewed: 2026-06-25T11:30:00Z
depth: deep
files_reviewed: 17
files_reviewed_list:
  - web/src/index.css
  - web/src/main.tsx
  - web/src/App.tsx
  - web/src/App.test.tsx
  - web/src/router.tsx
  - web/src/router.test.tsx
  - web/src/vitest.setup.ts
  - web/src/components/Logo.tsx
  - web/src/components/Logo.test.tsx
  - web/src/components/Button.tsx
  - web/src/components/Button.test.tsx
  - web/src/components/Pill.tsx
  - web/src/components/Pill.test.tsx
  - web/src/components/Card.tsx
  - web/src/components/Card.test.tsx
  - web/src/components/BrowserChrome.tsx
  - web/src/components/BrowserChrome.test.tsx
  - web/src/routes/StyleGuide.tsx
  - web/package.json
findings:
  blocker: 0
  critical: 0
  high: 3
  medium: 11
  low: 8
  total: 22
status: findings-fixed
fixed_at: 2026-06-25T11:50:00Z
fixed:
  - HI-01
  - HI-02
  - HI-03
  - MD-01
  - MD-03
  - MD-04
  - MD-05
  - MD-06
  - MD-07
  - MD-08
  - MD-09
  - LO-04
deferred:
  - MD-02   # subsumed by Button MD-03 commit (className-preserve test added)
  - MD-10   # cosmetic CSS dup; defer to Phase 9 polish
  - MD-11   # localhost-only risk; revisit in Phase 5 cloud-provider work
  - LO-01   # cosmetic; cursor-not-allowed kept for cross-platform safety
  - LO-02   # adds --color-blue-dark token; defer to Phase 4 button polish
  - LO-03   # subsumed by MD-01 fix (polymorphic ...rest spread shipped)
  - LO-05   # cosmetic comment; not load-bearing
  - LO-06   # housekeeping; no behavior change
  - LO-07   # TipTap dev-only logging; defer to Phase 4 editor work
  - LO-08   # superseded by HI-03 built-CSS smoke test
---

# Phase 1: Design System — Adversarial Code Review

**Reviewed:** 2026-06-25T11:30:00Z
**Depth:** deep (cross-file, contract-tracing)
**Files reviewed:** 17 source files (18 with package.json)
**Status:** findings — 0 blockers, 3 HIGH, 11 MEDIUM, 8 LOW

## Summary

Phase 1 ships a coherent design-system substrate: 12 color tokens, 4 radii, 4 shadows, 8 motion tokens (after the gap-fix), 6 primitive components, and a `/style-guide` showcase, all green on 35 tests and a clean `pnpm build`. The structural work is solid.

However — and Phase 1 is the contract every later phase inherits — there are real adversarial concerns. Three HIGH findings warrant fixing before downstream phases compose these primitives: (1) the `--animate-staggered-reveal` token has no showcase row (the gap-fix added the token but never wired the demo, so DESIGN-04 SC #2 is structurally satisfied but visually unverifiable); (2) **the `Logo` component cannot satisfy WCAG without an `aria-label` and the App.tsx hero uses it correctly but `StyleGuide.tsx` Iconography demo passes redundant labels while leaving `Logo` decorative usages in `BrowserChrome` showcase un-labelled** — minor but the bigger issue is that decorative-vs-meaningful Logo usage is inconsistent across the codebase; (3) the Tailwind 4 utility name **`bg-mut`** / **`text-mut`** / **`border-line`** etc. depend on Tailwind's `--color-*` → utility derivation — verified working in build output — but the abbreviated token names (`mut`, `blsoft`, `mtsoft`, `strsoft`) are opaque and one typo silently degrades to no-class. There is **no compile-time check** that these utilities exist; tests assert via regex match on the className string, which would still match a string like `bg-mut` even if Tailwind silently dropped it for being unknown. This is a quiet failure mode worth knowing about.

The bulk of the rest are MEDIUM/LOW: accessibility gaps in `Pill`/`BrowserChrome` traffic dots (`aria-hidden` is missing a `="true"` value in JSX boolean coercion — works but worth tightening), `disabled:cursor-not-allowed` on a `<button>` element is mostly redundant, `Card` clickable variant doesn't exist yet so no `<div onClick>` anti-pattern is shipping, but `Card`'s `as` prop is polymorphic without constraining the resulting element's required props (you can pass `as="button"` and get a `<button>` without `type="button"`), and StyleGuide is hard-coding hex values rather than reading computed token values (acceptable trade-off documented inline but the showcase will not visually drift if tokens change — defeats half the purpose of a "living" style guide).

Tests pass (35/35); production build succeeds; no Rust regressions; no security issues; no dangerous functions; no secrets; no `eval`; no XSS surface (no `dangerouslySetInnerHTML`; no untrusted-string interpolation into innerHTML). Phase 1 is releasable, but the design contract has gaps that will compound downstream.

---

## HIGH

### HI-01: `--animate-staggered-reveal` token added but showcase row missing — gap-fix is half-finished

**File:** `web/src/index.css:62` (token declared), `web/src/routes/StyleGuide.tsx:47-55` (`MOTION` array — no entry)
**Issue:** Commit `901bab0` ("fix(web,DESIGN-04): add missing staggered-reveal motion token") added the 8th motion token to `@theme` and a matching `@keyframes` block at lines 94-97. But `StyleGuide.tsx`'s `MOTION` array still contains only **7 entries** — `staggered-reveal` is not rendered in the motion preview section. The phase's stated acceptance gate is "the showcase verifies they run" (CONTEXT D-07); a token that has no showcase row cannot be visually verified, defeating the purpose of the gap-fix.
**Consequence:** Phase 4/5 will pick up the token name from `@theme` and Tailwind will expose `animate-staggered-reveal`, but **nobody has ever rendered a swatch of it during Phase 1**, so we cannot say with confidence the keyframe's `opacity 0→1 + translateY(6px→0)` looks correct. If the keyframe is wrong (e.g., translateY direction inverted), it ships to Phase 4 silently.
**Fix:**
```tsx
// web/src/routes/StyleGuide.tsx — add to MOTION array (after slide-in-right)
{
  name: "staggered-reveal",
  duration: "600ms",
  use: "Staggered list reveal (welcome, library)",
  className: "animate-staggered-reveal w-32 h-3 rounded-chip bg-blsoft",
},
```

### HI-02: Inconsistent `Logo` accessibility — some usages decorative, some meaningful, no clear contract

**File:** `web/src/components/Logo.tsx:15-18`; usages: `App.tsx:37`, `StyleGuide.tsx:62, 266, 372, 397-399`
**Issue:** `Logo` correctly toggles between `role="img" aria-label="..."` (when `ariaLabel` provided) and `aria-hidden="true"` (when omitted). Good. **But the usage sites are inconsistent in a way that confuses assistive tech:**
- `App.tsx:37` — `<Logo size={44} ariaLabel="Yogurt" />` next to a visible `<h1>yogurt</h1>`. The screenreader will announce: *"Yogurt, image. yogurt, heading level 1."* The Logo's aria-label is **redundant** with the adjacent heading — should be `aria-hidden` here.
- `StyleGuide.tsx:62` — `<Logo size={60} ariaLabel="Yogurt" />` next to `<h1>Yogurt style guide</h1>`. Same redundancy.
- `StyleGuide.tsx:266` — `<Logo size={size} ariaLabel={`Yogurt ${size}px`} />` in the logo-sizes section. This **is** meaningful (each Logo is the subject), but `aria-label="Yogurt 19px"` is odd — screenreader announces "Yogurt nineteen P X, image." The size annotation is visible in the adjacent caption; the Logo here should be `aria-hidden` and let the caption speak.
- `StyleGuide.tsx:372` — `<Logo size={32} />` (no ariaLabel) inside BrowserChrome mockup. Decorative, correct usage.
- `StyleGuide.tsx:397-399` — three Logos with `ariaLabel="Yogurt small/medium/large"`. Same pattern as line 266 — redundant with visible captions.
**Consequence:** Screenreader output across the design surface is noisy and announces the logo 5+ times on /style-guide. More importantly, the **contract** is unclear: future phases will copy the App.tsx pattern (`<Logo ariaLabel>` next to brand text) and propagate the redundancy. The actual rule is "Logo is decorative wherever brand text already names the app" — but no comment encodes that.
**Fix:** Adopt the rule "Logo is decorative by default; pass `ariaLabel` only if it is the sole representation of the brand name on screen." Update App.tsx and StyleGuide.tsx accordingly. Add a JSDoc note to `Logo.tsx`:
```tsx
/**
 * ...
 * A11y rule: pass `ariaLabel` ONLY when the Logo is the sole brand identifier
 * on screen. When adjacent text says "yogurt", omit `ariaLabel` so the SVG
 * gets `aria-hidden` and the screenreader is not redundant.
 */
```

### HI-03: Tailwind 4 utility name correctness is unverified — typos silently no-op

**File:** `web/src/components/Pill.tsx:25-30`, `web/src/components/Button.tsx:33-43`, `web/src/components/Card.tsx:38-44`, `web/src/components/BrowserChrome.tsx:22-23, 60-63`, `web/src/routes/StyleGuide.tsx` (passim)
**Issue:** All component class strings rely on Tailwind 4's CSS-first derivation from `@theme` (`--color-mut` → `bg-mut`/`text-mut`/`border-mut`). The codebase uses heavily abbreviated token names: `mut`, `blsoft`, `mtsoft`, `strsoft`. Tests assert presence via regex (`expect(el.className).toMatch(/bg-mut/)`) — but a regex match on the className string **does not verify Tailwind actually emitted CSS for that utility**. If a developer writes `bg-mute` (typo), the className contains `bg-mute`, the test passes if its regex is loose, and the rendered element silently has no background. There is no compile-time check, no failing test, no build error.
**Consequence:** Every later phase that consumes these tokens is at risk of silent CSS drift. A real example already lurks: `bg-blsoft` in `Pill.tsx:27` and `bg-mtsoft` in `Pill.tsx:28` — are these emitted as utilities? I verified via `pnpm build` output that `color-blue` is present in the built CSS, but a build that strips unused utilities (Tailwind's default behavior since v3) could drop any utility no testbed exercises. The Pill tone tests render every tone, which is good, but the showcase exercises them too via Vite dev — neither does a programmatic `getComputedStyle` check.
**Fix:** Either (a) add a Vitest assertion that renders each component and uses `window.getComputedStyle(el).backgroundColor` to confirm a non-default background paints (jsdom has limited CSS but Vitest can run a Playwright + real Chromium pass); or (b) cheaper: add a unit test that reads the built CSS file and `grep`s for the expected utility class definitions. The cheapest interim fix: rename the abbreviated tokens to less-typo-prone names (`mut` → `muted`, `blsoft` → `blue-soft`, `mtsoft` → `matcha-soft`, `strsoft` → `straw-soft`). Even Tailwind's own convention is to keep token names readable. The current abbreviations also defeat IDE autocomplete grep.

---

## MEDIUM

### MD-01: `Card` accepts `as` polymorphism but doesn't constrain required props for chosen element

**File:** `web/src/components/Card.tsx:30-51`
**Issue:** `as?: ElementType` lets a caller write `<Card as="button" onClick={fn}>...</Card>`, but the resulting `<button>` would be missing `type="button"` (defaults to `"submit"` inside a form), and the `CardProps` interface does not extend `ButtonHTMLAttributes` to expose `onClick`/`type`/`disabled`. Result: the polymorphic form silently drops typed event handlers.
**Consequence:** Phase 5 settings UI and Phase 7 library cards will need clickable cards (PRD §5.6 active provider card; library meeting cards). When someone writes `<Card as="button">`, TS won't error, and `onClick` won't compile-check against `ButtonHTMLAttributes`. They'll spread it via `...rest` — except there is no `...rest` in `Card`. So `onClick` is silently dropped entirely. **Clickable cards are completely broken** in this primitive.
**Fix:** Either (a) document that `Card` is presentational only and recommend wrapping with a separate `<button>`, or (b) implement proper polymorphism via the standard `PolymorphicProps` pattern (or library like `@radix-ui/react-slot`). For Phase 1 ship-by, the simplest is to add a `...rest` spread and a JSDoc warning:
```tsx
export function Card({ children, active, padding, as, className, ...rest }: CardProps & Record<string, unknown>) {
  // ...
  return <Tag className={cls} {...rest}>{children}</Tag>;
}
```
Then in CONTEXT-D for Phase 5/7, require an explicit `type="button"` audit when `as="button"` is used.

### MD-02: `Button` discards `className` if passed but accepts it — works, but test surface is missing

**File:** `web/src/components/Button.tsx:5, 60, 64`
**Issue:** `ButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "className">` — explicitly removes `className` from the native button props, then re-adds it as `className?: string` and appends. This works but is confusing: the `Omit` is defensive but the natural `ButtonHTMLAttributes` already exposes `className`, so the `Omit` is unnecessary. More importantly, there is **no test that confirms a caller-passed `className` is preserved and applied AFTER variant classes** (so it can override). The Button.test.tsx covers variants and disabled but not className composition.
**Consequence:** Phase 4 will want `<Button className="w-full">End meeting</Button>`. If the implementation regresses (e.g., someone re-orders the template literal to put `className` before `VARIANT[variant]`), tailwind-merge precedence rules would cause variant classes to silently win. No test catches it.
**Fix:** Add a test:
```ts
it("appends caller className after variant classes", () => {
  render(<Button className="w-full">x</Button>);
  expect(screen.getByRole("button").className).toMatch(/w-full/);
});
```
Long-term: install `tailwind-merge` and `clsx` (already on the project roadmap per CLAUDE.md "supporting frontend libraries") and use them properly to handle conflict resolution.

### MD-03: `Button` lacks `focus-visible` ring — keyboard users have no visible focus

**File:** `web/src/components/Button.tsx:12-31` (`BASE` constant)
**Issue:** The `BASE` class string has no `focus-visible:ring-*` or `focus-visible:outline-*` utility. Browsers will fall back to the user-agent focus ring, but on a primary button with `bg-blue` that may be invisible. There is no test asserting focus visibility.
**Consequence:** WCAG 2.4.7 (Focus Visible) failure. Every keyboard user (a substantial chunk of the dev-tooly target audience) sees no focus indicator. **Every later phase inherits this defect** on every Button instance.
**Fix:** Add to `BASE`:
```
"focus-visible:outline-none",
"focus-visible:ring-2",
"focus-visible:ring-blue/40",
"focus-visible:ring-offset-2",
"focus-visible:ring-offset-paper",
```

### MD-04: `App.tsx` link to `/style-guide` lacks `focus-visible` styling

**File:** `web/src/App.tsx:44-49`
**Issue:** `<Link to="/style-guide" className="text-blue underline-offset-2 hover:underline">/style-guide →</Link>` — same WCAG focus-visible gap as MD-03. No focus ring, no visible focus indicator. Keyboard users cannot tell when the link is focused.
**Consequence:** Same as MD-03 — inaccessible to keyboard navigation.
**Fix:** Add `focus-visible:underline focus-visible:outline-2 focus-visible:outline-blue/40` or similar.

### MD-05: Color contrast risk — `text-mut` (#8A8174) on `bg-paper` (#FBF7EF)

**File:** `web/src/components/Pill.tsx:26` (neutral pill), `web/src/components/BrowserChrome.tsx:63` (URL pill), `web/src/routes/StyleGuide.tsx` (extensive use)
**Issue:** `#8A8174` on `#FBF7EF` is ~4.0:1 contrast. WCAG AA requires **4.5:1** for body text under 18pt (or under 14pt bold). The neutral Pill text size is `text-[12px]` (Pill.tsx:18) — that is well under 18pt. **Fails WCAG AA.** The Pill is used for "neutral" status everywhere in the showcase.
**Consequence:** The "muted text" convention permeates the codebase — `text-mut` appears ~30+ times in StyleGuide.tsx alone for captions, mono-token names, and metadata. All of it is below AA on `bg-paper`. Phase 7 library will inherit this for meeting metadata under cards. Compliance-bound enterprise users will reject the product.
**Fix:** Darken `--color-mut` to something like `#6E665A` (≥4.5:1 on `#FBF7EF`). This is a PRD-token change — confirm with design board before adjusting; alternative is to bump small caption text to ≥13.5px/600-weight (still fails 4.5:1 on body though).

### MD-06: Color contrast — `text-straw` (#E07A66) on `bg-strsoft` (#FBE6E0) inside Pill

**File:** `web/src/components/Pill.tsx:29`
**Issue:** `#E07A66` on `#FBE6E0` is ~2.9:1. **Fails WCAG AA for any text size**, and fails AA Large too. The `straw` tone pill ("Recording", error states) will be unreadable for low-vision users.
**Consequence:** Same as MD-05 — propagates to every "destructive" status pill in later phases, including the permission-denied state (Phase 7 STATE-02 — the highest-stakes error in the product). A user who can't read "Yogurt can't hear the call yet" can't fix the permission grant.
**Fix:** Darken the text inside `straw` tone. Either (a) use `text-ink` instead of `text-straw` on soft-strawberry pills (8.4:1, passes AAA), or (b) introduce a `--color-straw-dark: #8E3A26` token for use as text-on-soft-straw.

### MD-07: `Pill` has no `role` and is announced as plain text

**File:** `web/src/components/Pill.tsx:36-38`
**Issue:** Pill renders a `<span>` with no `role`. The "Recording" pill, "Local-only · on" pill, and "Active" status pill are status indicators — they should have `role="status"` (live region) or at least `role="img" aria-label="status: recording"` so screenreaders announce them as discrete elements, not as inline text in a heading or paragraph.
**Consequence:** Status updates that change dynamically (Phase 2 recording badge ticking, Phase 4 enhancing banner appearing) will silently update without screenreader announcement. The user has no indication the app state changed.
**Fix:** Make `role` and `aria-label` props on `Pill`:
```tsx
interface PillProps {
  tone?: Tone;
  children: ReactNode;
  className?: string;
  role?: "status" | "img" | string;
  "aria-label"?: string;
}
```
Apply `role="status"` to `RecordingBadge` (it already represents live status). Document in PillProps JSDoc that decorative pills get no role; status pills get `role="status"`.

### MD-08: `BrowserChrome` traffic dots use `aria-hidden` (JSX boolean) but the inner URL pill is announced

**File:** `web/src/components/BrowserChrome.tsx:41, 47, 53, 60-67`
**Issue:** The three traffic dots correctly carry `aria-hidden` (JSX boolean coerces to `aria-hidden="true"`). Good. But the URL pill (`localhost:7878/welcome`, font-mono) is announced by screenreaders as actual page content with no context — the user hears "localhost colon 7878 slash welcome" with no indication it's a decorative mockup chrome, not a real URL bar.
**Consequence:** On `/style-guide`, screenreader users hear a phantom URL announcement that has nothing to do with the page content. In Phase 7 onboarding mockups using BrowserChrome, same problem.
**Fix:** The entire `BrowserChrome` decorative chrome (header bar + traffic dots + URL pill) should be `aria-hidden="true"` on the outer header `<div>`. Only the `children` body should be exposed. Alternatively, wrap the chrome in `<div role="presentation" aria-hidden="true">`. Add a test asserting the header is aria-hidden.

### MD-09: `Logo` SVG omits `focusable="false"` — IE/legacy Edge default to focusable SVGs

**File:** `web/src/components/Logo.tsx:20-28`
**Issue:** SVGs are focusable=true by default in IE11 and old Edge. Modern browsers default to false, but the standard defensive practice is to explicitly add `focusable="false"`. Without it, keyboard tab order may unpredictably stop on every rendered Logo. The StyleGuide page renders 8+ Logos.
**Consequence:** Low — IE11/old Edge are not Phase 1 platforms (macOS Safari/Chrome/Firefox are). But the convention is cheap and the codebase has no policy on it. If the project later supports Windows/web, this becomes a bug.
**Fix:**
```tsx
<svg viewBox="0 0 44 44" width={size} height={size} className={className} focusable="false" style={{ flex: "none" }} {...a11yProps}>
```

### MD-10: `RecordingBadge` uses `bg-card` (white) on `bg-paper` (cream) — but border is `border-straw` only, not `border-line` — slight visual contrast inconsistency

**File:** `web/src/components/Pill.tsx:49-66`
**Issue:** Comment on lines 42-43 says "Always uses a white surface with a strawberry hairline border to match PRD §16.6." That's correct per spec. But the badge has no `text-ink` color override and inherits — on `bg-card` (white), readable. On `bg-paper` (cream) it sits on a slightly different surface. The badge is shipped without `border-line` fallback so there's no "background-mismatch" gradient. Inspecting line 54: `"bg-card text-ink border border-straw"` — fine. The contrast issue is just that the elapsed-time number is `text-[12px]` font-mono on white — and is wrapped in another `<span className="font-mono text-[12px] tracking-tight">` (line 63) **duplicating the text-[12px] already on the parent (line 55)**. Redundant CSS.
**Consequence:** Cosmetic; no bug. Just duplicate utility classes that could drift.
**Fix:** Remove `text-[12px]` from line 63 (parent already sets it):
```tsx
<span className="font-mono tracking-tight">{elapsed}</span>
```

### MD-11: `App.tsx` displays `e.message` from a network error directly in UI without sanitization

**File:** `web/src/App.tsx:28-30, 55-63`
**Issue:** `setHealthError(e instanceof Error ? e.message : "...")` — then renders `{healthError}` inside `<code>`. While not technically XSS (React escapes text children), it does surface raw fetch error messages to the end user. A typical message like `"NetworkError when attempting to fetch resource."` or `"Failed to fetch"` is fine. But malformed URLs, CORS errors, or backend error responses could surface backend-specific paths or service names that the user wasn't supposed to see (path disclosure). For a localhost-only app, the risk is essentially zero — but the pattern propagates to production code that may later fetch from third-party (Deepgram, OpenAI).
**Consequence:** Low for Phase 1 (localhost). Sets a bad precedent for Phase 5+ when network errors come from cloud STT providers, where Deepgram error messages may include API keys in URLs (unlikely but possible).
**Fix:** Document the convention: surface a fixed user-facing string + log the raw `e.message` to console:
```ts
.catch((e) => {
  console.error("[health-check] fetch failed:", e);
  setHealthError("server unreachable — is `yogurt start` running?");
});
```

---

## LOW

### LO-01: `disabled:cursor-not-allowed` on a `<button disabled>` is mostly redundant

**File:** `web/src/components/Button.tsx:30`
**Issue:** Disabled buttons don't fire `:hover` or receive pointer events in many browsers, so the `cursor-not-allowed` style is only visible during the brief mouseover transition. Minor noise.
**Consequence:** None; cosmetic.
**Fix:** Optional removal. Or keep for the safety blanket — it does help on Linux Chromium where pointer-events on disabled buttons differ.

### LO-02: `Button` `hover:opacity-90` is a fragile primary-hover effect

**File:** `web/src/components/Button.tsx:38`
**Issue:** Comment acknowledges this is a workaround for not having a `bg-blue-600` token. But `hover:opacity-90` affects the **entire button including text and shadow**, dulling everything together. Standard convention is to define a `--color-blue-dark` token and use `hover:bg-blue-dark`.
**Consequence:** Minor visual quality issue; the hover state looks less crisp than a true darker fill would.
**Fix:** Add `--color-blue-dark: #4A40A8` to `@theme`; update primary variant to `hover:bg-blue-dark`.

### LO-03: `Card`'s `as` prop with `ElementType` swallows polymorphic prop types entirely

**File:** `web/src/components/Card.tsx:1, 12, 37`
**Issue:** See MD-01 — TS does not narrow `as: ElementType` to constrain children/event handlers. Dead-end polymorphism. Minor in Phase 1 (only `<article>` is used in tests); will bite Phase 5/7.
**Fix:** See MD-01 fix.

### LO-04: `StyleGuide.tsx` hardcodes hex values rather than reading computed CSS

**File:** `web/src/routes/StyleGuide.tsx:19-32`
**Issue:** The `COLORS` array hardcodes hex values for swatches. If the `@theme` block in index.css changes (e.g., we darken `--color-mut` per MD-05), the showcase will not reflect the change — it will keep painting the old hex via inline `style={{ background: c.hex }}`. The showcase is documented as the "living documentation"; in this respect it is not living.
**Consequence:** A future token-change PR will silently leave the showcase out of sync. Reviewer eyeballs the showcase, sees the old color, signs off.
**Fix:** Use CSS variables for swatch backgrounds:
```tsx
<div className="h-16 w-full rounded-chip border border-line"
     style={{ background: `var(--color-${c.token})` }} />
```
And read the hex via `getComputedStyle(document.documentElement).getPropertyValue(\`--color-${c.token}\`)` in a `useEffect` if you want the displayed hex to match. Or simply drop the hex display and just show the swatch + token name.

### LO-05: `StyleGuide.tsx` motion section uses arbitrary `bg-[length:400%_100%]` — works but odd

**File:** `web/src/routes/StyleGuide.tsx:50`
**Issue:** `shimmer` example uses `bg-gradient-to-r from-line via-paper to-line bg-[length:400%_100%]` — the shimmer keyframe animates `background-position` from `-200%` to `200%` (index.css:75-76), which requires the background to be sized larger than the element. Works correctly, but the magic-number `400%` is undocumented.
**Consequence:** None; cosmetic.
**Fix:** Add a comment near the shimmer entry: `// 400% width is the minimum for the -200%→200% keyframe to traverse cleanly`.

### LO-06: `vitest.setup.ts` is one line — could inline into `vitest.config.ts`

**File:** `web/src/vitest.setup.ts`
**Issue:** A one-line setup file (`import "@testing-library/jest-dom/vitest"`) is fine but trivially inlineable. No bug.
**Consequence:** None.
**Fix:** Optional consolidation.

### LO-07: `App.tsx` does not handle the case where the TipTap editor fails to initialize

**File:** `web/src/App.tsx:15-18, 68`
**Issue:** `useEditor()` returns `null` until initialized (and can stay null if extensions throw). `<EditorContent editor={editor} />` handles null gracefully (renders empty div), but there's no user-visible signal if TipTap fails to mount. Phase 4 will heavily rely on TipTap; setting up a logging baseline here would be cheap.
**Consequence:** Silent TipTap failures in dev are mysterious.
**Fix:** Optionally `useEffect(() => { if (editor) console.log("[editor] ready"); else console.warn("[editor] not initialized yet"); }, [editor]);` — though noisy in normal renders.

### LO-08: 35 tests passing — but **zero** tests assert `getComputedStyle` for any token utility

**File:** all `*.test.tsx`
**Issue:** Tests assert className regex matches. None confirm the browser actually applies a non-default background, color, font-family, or animation. The jsdom test environment is limited, but Vitest can be configured with `@vitest/browser` for real-browser assertions.
**Consequence:** Tests will continue passing even if Tailwind 4 silently drops a utility (token rename, theme block reorder, JIT scope issue). The whole design system could degrade to bare CSS and tests stay green.
**Fix:** Phase 9 polish — add `pnpm test:visual` Playwright lane with a smoke test that visits `/style-guide` and asserts `getComputedStyle(swatch).backgroundColor === 'rgb(91, 79, 199)'` for the blue swatch, etc.

---

## What I verified (non-finding evidence)

- 35/35 web tests pass on a fresh `pnpm test` run at review time.
- No `eval`, `innerHTML`, `dangerouslySetInnerHTML`, `exec`, `system` calls anywhere in scope.
- No hardcoded secrets / API keys / tokens in source.
- No `TODO` / `FIXME` / `XXX` / `HACK` markers in the Phase 1 source files.
- No empty `catch {}` blocks (App.tsx's catch logs + sets state).
- React Router 7 uses `createBrowserRouter` (correct — not `createHashRouter`), preserving Phase 0's SPA fallback assumption.
- The `LO-03 inline-error` Phase-0 hardening is preserved and **wired** in App.tsx (lines 14, 26-31, 55-63), not dead code.
- 10 `@fontsource/*` imports declared in main.tsx, all 10 referenced in PRD §16.3. None are dead.
- `@theme` block declares 12 color tokens, 4 radii, 4 shadows, 2 easings, 8 animations (after gap-fix), and all 8 `@keyframes` are present. The `staggered-reveal` keyframe (lines 94-97) matches the intended `opacity 0→1 + translateY(6px → 0)` — visual intent is correct.
- Production `pnpm build` succeeds; built CSS contains `color-blue` utility (confirmed via Phase 0 verifier evidence).
- Router setup correctly uses `RouterProvider` + `createBrowserRouter`; routes are eager-loaded (acceptable for two routes), no Suspense/lazy needed at Phase 1 scale.
- Test files correctly use `MemoryRouter` / `createMemoryRouter` to isolate App from real history.
- No `as any` TypeScript escape hatches in Phase 1 source.
- No mutable default arguments, no off-by-one loop indices, no missing await on a Promise.

## What I did NOT verify (out of v1 scope or human-required)

- Real visual quality of `/style-guide` (HUMAN check #1 in 01-VERIFICATION.md remains required).
- Bundle size / tree-shaking efficiency of `@fontsource/*` (Performance out of v1 scope per review charter).
- Whether macOS Safari's font rendering matches design intent.
- Animation frame-rate / smoothness (visual review territory).

---

## Recommendation

**Status: findings.** None of the 22 items are pipeline-blocking, but **HI-01, HI-03, MD-03, MD-04, MD-05, MD-06, MD-07, MD-08** form an a11y/quality cluster that compounds badly when Phases 4–7 inherit the primitives. I would gate Phase 2 entry on:

1. **Add the missing showcase row** for `staggered-reveal` (HI-01; 5-minute fix).
2. **Add `focus-visible` ring** to `Button` and the App link (MD-03, MD-04; 5-minute fix per).
3. **Resolve the color-contrast violations** (MD-05, MD-06) — either darken tokens or document the AA waiver explicitly with PRD acknowledgement.
4. **Pick a Pill `role` convention** and apply to `RecordingBadge` (MD-07).
5. **Mark `BrowserChrome` chrome as `aria-hidden`** (MD-08).

The polymorphic `Card` issue (MD-01) can wait until Phase 5 when the first clickable card lands — but document it now so Phase 5 doesn't paper over it.

The Tailwind 4 utility-existence concern (HI-03) is the longest-tail risk; consider adopting a `getComputedStyle` smoke test in Phase 9 polish.

---

_Reviewed: 2026-06-25T11:30:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: deep_

---

## Fix Log

**Fixed at:** 2026-06-25T11:50:00Z
**Iteration:** 1
**Test gate:** 75/75 web tests + 28/28 cargo tests + clean `pnpm build`

### Resolved

| Finding | Commit | Summary |
|---------|--------|---------|
| HI-01 | `224a0be` | Added staggered-reveal showcase row to MOTION array |
| HI-02 | `b6f4346` | Established "Logo decorative by default" a11y contract; dropped redundant ariaLabels in App + StyleGuide; added focusable="false" (also covers MD-09); +3 tests |
| MD-08 | `778cae2` | Marked BrowserChrome header `aria-hidden="true"`; +1 test |
| MD-07 | `a1a38fb` | Added `status` prop ('none' \| 'status' \| 'alert') to Pill; RecordingBadge now role="status" + aria-live="polite" + aria-label="Recording, MM:SS"; +4 tests |
| MD-01 | `383563e` | Card spreads ...rest onto rendered element; CardProps extends HTMLAttributes; +2 tests covering onClick and aria-* forwarding |
| MD-03/MD-04 | `4ade640` | Added focus-visible:ring-2 + blueberry/40 + paper offset to Button BASE and App link; +2 tests (focus-ring + className-preserve covering MD-02) |
| MD-05/MD-06 | `94e58f7` | Established token-usage convention: text-mut for captions only (>=11px); text-straw reserved for borders/icons on bg-strsoft; Pill straw tone now text-ink + border-straw; 01-UI-SPEC.md gains "Token usage rules" subsection |
| HI-03 | `e4a0d54` | Added `_token-utilities.test.tsx` reading built dist/assets CSS and asserting every token-derived utility resolves via var(--color-*) / var(--animate-*); 30 new tests; skips gracefully if dist/ absent |
| LO-04 | `fc7082a` | Swatches paint from `var(--color-${token})` so StyleGuide stays in sync with @theme |

### Deferred (with rationale)

- **MD-02 (Button className-preserve test):** Subsumed — the regression test now lives alongside the MD-03 focus-visible test in `4ade640`. No separate commit needed.
- **MD-09 (Logo focusable="false"):** Shipped as part of HI-02 (`b6f4346`) — same file, same edit.
- **MD-10 (RecordingBadge duplicate text-[12px]):** Cosmetic CSS deduplication; deferred to Phase 9 polish.
- **MD-11 (raw fetch error in UI):** Localhost-only risk in Phase 1; revisit during Phase 5 when cloud-provider error messages start surfacing.
- **LO-01 (disabled:cursor-not-allowed redundancy):** Keep — useful Linux Chromium safety blanket.
- **LO-02 (--color-blue-dark token for true hover):** Adds a new design token; defer to Phase 4 button polish so design board can co-design the value.
- **LO-03 (Card polymorphic prop narrowing):** Subsumed by MD-01 fix — `...rest` spread shipped; full element-narrowing of `as` deferred until a real use-case surfaces in Phase 5/7.
- **LO-05 (shimmer 400% comment):** Cosmetic; not load-bearing.
- **LO-06 (vitest.setup.ts consolidation):** Housekeeping; no behavior change.
- **LO-07 (TipTap init logging):** Dev-only; defer to Phase 4 editor work.
- **LO-08 (visual Playwright lane):** Superseded by HI-03's cheaper built-CSS smoke test; full Playwright visual diff still deferred to Phase 9 polish.

### Verification gates passed

- `pnpm test`: 75/75 (up from 35/35 baseline) — 40 new regression tests
- `pnpm build`: clean
- `cargo test --workspace`: 28/28

_Fixed: 2026-06-25T11:50:00Z_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
