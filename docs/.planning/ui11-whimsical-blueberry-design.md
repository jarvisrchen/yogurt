# UI-11: restyle yogurt to Whimsical × Blueberry

Design and implementation plan, not yet implemented.
Ticket: `docs/TODO.md` UI-11.
Lavish review surface: [`../.lavish/ui11-whimsical-blueberry-design.html`](../.lavish/ui11-whimsical-blueberry-design.html).
Mockups: [`../.lavish/design-mockups/whimsical-blueberry.html`](../.lavish/design-mockups/whimsical-blueberry.html) and [`whimsical-blueberry-dark.html`](../.lavish/design-mockups/whimsical-blueberry-dark.html), tokens from [Whimsical on Refero Styles](https://styles.refero.design/style/8e153a14-40a9-4793-b94b-c144d325c730).

Richard picked this direction: Whimsical's type, radii and soft shadows, on yogurt's own Blueberry palette, in both light and dark.
Functionality does not change.
Every route, component, API call, keyboard shortcut and test keeps its behavior.
This is a restyle of the design tokens plus a cleanup of the call sites that currently bypass those tokens.

## Why this is mostly a token change

The app was built on Tailwind 4's CSS-first `@theme` in `web/src/index.css`.
Every color, radius, shadow and font family is a `--color-*`, `--radius-*`, `--shadow-*` or `--font-*` variable, and Tailwind emits `var(--…)` in every utility.
Dark mode (UI-6) already works by re-pointing the color tokens under `:root[data-theme="dark"]`, so components never hardcode palette hex.
That same seam is how this restyle lands: change the token values once and the 69 non-test components follow.

What does not follow for free is the set of call sites that reach past the tokens.
The survey below lists them; the plan migrates them onto tokens first so the flip is complete.

## What changes, and what does not

Unchanged, on purpose:

- Every color token, light and dark. The palette is the yogurt identity and is the whole reason this variant exists.
- The logo (`Logo.tsx`), the label chip palette (lilac, honey, slate), the AI-grey swatch contract and the transcript-link style.
- Layout, spacing, the 660px hero column, the 330px transcript dock, all motion and keyframes.
- JetBrains Mono for the mono role.

Changed:

| Token | Today | Target | Where it shows |
| --- | --- | --- | --- |
| `--font-sans` | Hanken Grotesk | Manrope, fallbacks Inter, DM Sans, system-ui | everything |
| heading weight | 700 (`font-bold`) | 800 (`font-extrabold`) | all `heading-*` utilities |
| heading tracking | `tracking-tight` (-0.025em) | -0.01em | all `heading-*` utilities |
| UI letter-spacing | none | 0.009em on body | `html, body` |
| `--radius-chip` | 6px | 8px | label chips, kbd hints |
| `--radius-button` | 9px | 12px | buttons, inputs, nav rows |
| `--radius-card` | 14px | 16px | cards, meeting cards, dialogs |
| new `--radius-panel` | - | 24px | dock, chat window, dialogs that float |
| `--radius-pill` | 999px | 999px | unchanged |
| `--shadow-card` | `0 2px 6px rgba(40,30,15,.08)` | `0 16px 32px -4px rgba(33,29,24,.06)` | cards |
| `--shadow-pop` | `0 12px 30px -10px rgba(40,30,15,.22)` | `0 12px 16px -4px rgba(33,29,24,.18)` | popovers, ask pill, recording pill |
| `--shadow-window` | `0 26px 60px -28px rgba(40,30,15,.4)` | `0 24px 48px -8px rgba(33,29,24,.22)` | dialogs, dock |
| dark shadows | none (light values reused) | same offsets, `rgba(0,0,0,.35)` / `.5` / `.6` | dark mode only |
| `Button` padding | `px-4 py-2`, 13.5px, 600 | `px-[18px] py-2.5`, 14px, 600 | all buttons |
| `Card` active border | 1.5px blueberry | 1.5px blueberry | unchanged |

Shadows get a dark override for the first time.
Today the light shadow values render under dark mode, which is invisible on `#1B1815`, so the new depth would be lost without it.

## Survey: call sites that bypass tokens

Counted across the 69 non-test `.tsx` files in `web/src`.

Radii off-token.
14 files use Tailwind's default `rounded-md`, `rounded-lg`, `rounded-xl`, `rounded-2xl` or a literal like `rounded-[10px]` instead of `rounded-chip`, `rounded-button`, `rounded-card`.
Most are in `components/settings/` (ProviderRow, ProviderCard, GeneralSection, AudioSection, AddProviderForm, STTPicker, ComboBox), plus MeetingPost, ChatWindow, ChatMessage, TemplatePicker, ModelDownloadDialog, Settings, Library.
Mapping: `rounded-md` → `rounded-chip`, `rounded-lg` → `rounded-button`, `rounded-xl` → `rounded-card`, `rounded-2xl` → `rounded-panel`, literals case by case.

Shadows off-token.
10 sites use a literal `shadow-[…]`, all of them restating an existing token: 3× the blueberry button glow, 2× `shadow-window`, 1× `shadow-pop`, and 4 hairline shadows that map to `shadow-card`.
Replace each with the token utility.

Headings off-utility.
9 files re-type `font-bold tracking-tight text-[Npx]` inline instead of a `heading-*` utility: Meeting (2), MeetingPost (2), Settings, Welcome, MeetingCard, Greeting, ModelDownloadStub, ModelDownloadDialog, StyleGuide (3).
Move each to the nearest utility, adding `heading-greeting` (34px) and `heading-title` (30px) for the two one-off display sizes the comment in `index.css` currently leaves inline.
After that, weight and tracking are set in one place.

Raw hex in components.
Only `BrowserChrome.tsx` (macOS traffic lights) and the `style={{ color: "#FFFFFF" }}` on two buttons are real hex.
The rest are comments or the logo.
Nothing to migrate for color.

Tests that assert classes.
9 test files use `toHaveClass`, all on color tokens (`text-blue`, `text-matcha`, `bg-blsoft`).
No test asserts a radius, shadow or font, so the token flip breaks no test.
Migrating `rounded-*` call sites is also test-neutral for the same reason.

## Fonts

Manrope replaces Hanken Grotesk through the same mechanism already in place: `@fontsource/manrope` side-effect imports in `web/src/main.tsx`, weights 400, 500, 600, 700, 800.
Drop `@fontsource/hanken-grotesk` from `package.json`.
Manrope is OFL 1.1, same license family as Hanken Grotesk, so `THIRD_PARTY_LICENSES.md` needs its row swapped when that file is regenerated.
No CDN, no network fetch, consistent with the zero-telemetry constraint.

Manrope sets slightly wider than Hanken at the same size.
The places to check for wrapping are the ones that were already tight: provider rows (UI-1 fixed an overlap there), meta pills, the Settings nav, the `⌘N` kbd hint in the New meeting button.

## Component by component

`Button.tsx`: padding and size per the table, radius follows the token.
The `primary` variant keeps `shadow-button-blue`, see decision 1.

`Card.tsx`: no code change, radius and shadow follow tokens.

`Pill.tsx`: no code change, `rounded-pill` unchanged.
Whimsical uses 1px borders and so does yogurt already.

`Sidebar.tsx`, `SidebarNav.tsx`: nav rows use `rounded-button` (12px) for the active `bg-blsoft` state, matching the mock.
The brand wordmark uses `heading-wordmark`, which picks up weight 800.

`TranscriptDock.tsx`, `ChatWindow.tsx`: outer panel takes `rounded-panel` on the exposed corner and `shadow-window`.

`RecordingPill.tsx`, `AskPill.tsx`, `MeetingDetectedBanner.tsx`: floating, take `shadow-pop`.

`EnhancingBanner.tsx`: unchanged, it already uses `bg-blsoft` and the mono counter.

`YogurtEditor` (`editor/index.tsx`, `.yogurt-editor` in `index.css`): body copy follows `--font-sans` automatically.
Prose sizes (24 / 19 / 16px headings at lines ~392-403 of `index.css`) get the 800 weight and -0.01em tracking to match the new heading scale.

`StyleGuide.tsx`: update its own three inline headings to utilities, and update the section that documents radii and elevation with the new values.
This route is the visual regression surface for the PR.

`web/index.html`: `theme-color` stays `#5B4FC7`.

## Rollout: two PRs

PR A, "UI-11a: move off-token styles onto tokens".
Pure refactor, zero intended visual change.
Migrates the 14 radius files, 10 shadow literals and 9 inline headings onto tokens and utilities, adds `--radius-panel`, `heading-greeting`, `heading-title`.
Reviewable by diff alone, and Playwright screenshots before and after should be pixel-identical.

PR B, "UI-11b: flip tokens to Whimsical × Blueberry".
Changes `index.css` token values, the `heading-*` utilities, `Button` sizing, the font imports and `package.json`.
Small diff, whole-app visual change.
PR body carries Playwright screenshots of the five routes in light and dark next to the mockups.

Splitting this way means the visual review of PR B is not polluted by 30 mechanical edits, and PR A cannot regress anything visible.
Both stay under `web/`, so both qualify for the cloud-session path in `AGENTS.md`.

## Verification

- `just test` green on both PRs, no test edits expected.
- Playwright screenshots of `/welcome`, `/`, `/meeting/:id`, `/meeting/:id/post`, `/settings/model` in light and dark for PR B, attached to the PR body.
- Manual pass on the tight layouts listed under Fonts, at a 1024px window.
- Contrast: unchanged colors, so the UI-7 AI-grey and Pill contrast notes still hold.
- Dark mode toggle in Settings → General still switches without a flash, since the inline script in `index.html` is untouched.

## Decisions needed

1. Blueberry button glow.
   Whimsical is flat, no glow on primary buttons.
   Today's primary keeps a `shadow-button-blue` glow.
   Recommendation: keep it but soften to `0 8px 16px -6px rgba(91,79,199,.35)`, the one place yogurt stays a little more playful than Whimsical.
2. Heading weight.
   Mock uses Manrope 800.
   700 reads calmer at 15px body.
   Recommendation: 800 for `heading-hero`, `heading-greeting`, `heading-title`, `heading-section`; 700 for `heading-card`, `heading-sm`, `heading-wordmark`.
3. Body size.
   The mock rendered at 14px, the app is 15px.
   Recommendation: stay at 15px, Manrope is compact enough and no layout needs to move.
4. `rounded-full` on 22 sites.
   These are circles (dots, avatars, icon buttons) and should stay `rounded-full`, not migrate to `rounded-pill`.
   Calling it out so PR A does not over-migrate.

## Out of scope

- No new components, no layout changes, no copy changes.
- Settings audio and general sections were not mocked; they inherit the tokens like everything else.
- The Strawberry and Matcha alternate themes deferred in PRD §16.2 stay deferred.
