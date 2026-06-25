/**
 * Style guide — visual proof that every PRD §16 token and every Phase-1
 * component primitive is wired correctly. A single long scrollable page;
 * intentionally no nav. Section helper at the bottom keeps headings
 * consistent.
 *
 * Iconography note (DESIGN-06): Phase 1 demonstrates the icon strategy
 * with inline SVGs (Logo, BrowserChrome traffic lights) and Unicode
 * glyphs (⌘ ✓ + → ↳ ⌄ ✨ ⚙). Full Lucide vs Phosphor library selection
 * is deferred to Phase 7 per CONTEXT D-13.
 */
import type { ReactNode } from "react";
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

const RADII: Array<{ name: string; px: number; bg: string }> = [
  { name: "chip",   px: 6,   bg: "bg-blsoft" },
  { name: "button", px: 9,   bg: "bg-blsoft" },
  { name: "card",   px: 14,  bg: "bg-card border border-line" },
  { name: "pill",   px: 999, bg: "bg-mtsoft" },
];

const SHADOWS: Array<{ name: string; className: string; use: string }> = [
  { name: "shadow-card",   className: "shadow-card",                                                       use: "Card surface (default)" },
  { name: "shadow-pop",    className: "shadow-[0_12px_30px_-10px_rgba(40,30,15,0.22)]",                    use: "Chat window, popovers" },
  { name: "shadow-window", className: "shadow-[0_26px_60px_-28px_rgba(40,30,15,0.4)]",                     use: "Modals, mockup chrome" },
];

const MOTION: Array<{ name: string; duration: string; use: string; className: string }> = [
  { name: "recpulse",       duration: "1.4s",  use: "Recording / enhancing dot pulse",       className: "animate-recpulse w-3 h-3 rounded-pill bg-straw" },
  { name: "blink",          duration: "1.0s",  use: "Cursor blink for active editor",         className: "animate-blink w-[2px] h-5 bg-ink" },
  { name: "shimmer",        duration: "1.25s", use: "Skeleton placeholders during enhance",   className: "animate-shimmer w-32 h-3 rounded-chip bg-gradient-to-r from-line via-paper to-line bg-[length:400%_100%]" },
  { name: "wave",           duration: "1.0s",  use: "3-bar audio wave on transcript tab",     className: "animate-wave inline-block w-1 h-4 bg-blue rounded-pill origin-center" },
  { name: "float",          duration: "3.5s",  use: "Empty-state logo gentle float",          className: "animate-float inline-block" },
  { name: "pop-up",         duration: "260ms", use: "Chat window expanding from Ask pill",    className: "" },
  { name: "slide-in-right", duration: "340ms", use: "Live transcript dock opening",           className: "" },
  { name: "staggered-reveal", duration: "600ms", use: "Staggered list reveal (welcome, library)", className: "animate-staggered-reveal inline-block w-32 h-3 rounded-chip bg-blsoft" },
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
            Every token and component primitive defined in PRD §16, rendered
            as living documentation. If anything below looks wrong, the
            design system is wrong — fix it here.
          </p>
        </div>
      </header>

      {/* ─── Color tokens ─────────────────────────────────────────────── */}
      <Section
        title="Color tokens (PRD §16.2)"
        caption="Blueberry theme — the only v1 theme."
      >
        <div className="grid grid-cols-2 gap-4 md:grid-cols-3">
          {COLORS.map((c) => (
            <Card key={c.token} padding="sm">
              <div
                className="h-16 w-full rounded-chip border border-line"
                style={{ background: c.hex }}
                aria-label={`Swatch for --color-${c.token}`}
              />
              <div className="mt-3">
                <div className="font-mono text-[11px] text-mut">
                  --color-{c.token}
                </div>
                <div className="font-mono text-[12px] text-ink">{c.hex}</div>
                <div className="mt-1 text-[12px] text-mut leading-snug">
                  {c.use}
                </div>
              </div>
            </Card>
          ))}
        </div>
      </Section>

      {/* ─── Typography ───────────────────────────────────────────────── */}
      <Section
        title="Typography (PRD §16.3)"
        caption="Three families. Use them deliberately."
      >
        <Card padding="lg">
          <div className="space-y-6">
            <div>
              <p className="font-mono text-[11px] text-mut uppercase tracking-wide">
                Instrument Serif · 52 / 38 / 30
              </p>
              <p className="font-serif text-[52px] leading-none text-ink mt-1">
                yogurt
              </p>
              <p className="font-serif text-[38px] leading-tight text-ink">
                Welcome to yogurt.
              </p>
              <p className="font-serif text-[30px] leading-tight text-ink">
                Good afternoon, Dana
              </p>
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
              <p className="font-mono text-[11px] text-blue">
                localhost:7878/welcome
              </p>
              <p className="font-mono text-[12px] text-ink">00:11:02</p>
            </div>
          </div>
        </Card>
      </Section>

      {/* ─── Spacing ──────────────────────────────────────────────────── */}
      <Section
        title="Spacing (PRD §16.4)"
        caption="4-base scale. Nothing off-scale."
      >
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

      {/* ─── Border radius ────────────────────────────────────────────── */}
      <Section
        title="Border radius (PRD §16.4)"
        caption="Four scales — pick the right one."
      >
        <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
          {RADII.map((r) => (
            <Card key={r.name} padding="sm">
              <div
                className={`h-20 w-full ${r.bg}`}
                style={{ borderRadius: r.px }}
                aria-label={`${r.name} radius example`}
              />
              <div className="mt-3">
                <div className="font-mono text-[11px] text-mut">
                  rounded-{r.name}
                </div>
                <div className="font-mono text-[12px] text-ink">{r.px}px</div>
              </div>
            </Card>
          ))}
        </div>
      </Section>

      {/* ─── Elevation ────────────────────────────────────────────────── */}
      <Section
        title="Elevation (PRD §16.4)"
        caption="Three depths. Use sparingly."
      >
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

      {/* ─── Motion ───────────────────────────────────────────────────── */}
      <Section
        title="Motion (PRD §16.5)"
        caption="Each animation, live. Don't add new ones."
      >
        <Card padding="md">
          <div className="space-y-4">
            {MOTION.map((m) => (
              <div key={m.name} className="flex items-center gap-4">
                <div className="w-40">
                  <div className="font-mono text-[11px] text-mut">{m.name}</div>
                  <div className="font-mono text-[12px] text-ink">
                    {m.duration}
                  </div>
                </div>
                <div className="w-40 h-8 flex items-center">
                  {m.className ? (
                    <span className={m.className} aria-hidden />
                  ) : (
                    <span className="text-[11px] text-mut italic">
                      scripted — see chat / transcript
                    </span>
                  )}
                </div>
                <div className="flex-1 text-[12px] text-mut">{m.use}</div>
              </div>
            ))}
          </div>
        </Card>
      </Section>

      {/* ─── Logo at multiple sizes ──────────────────────────────────── */}
      <Section
        title="Logo (PRD §16.1)"
        caption="Renders cleanly from 19px to 60px."
      >
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

      {/* ─── Buttons ──────────────────────────────────────────────────── */}
      <Section
        title="Buttons (PRD §16.6)"
        caption="Three variants. Primary is the action; secondary is the alternative; ghost is the dismissal."
      >
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
              <Button variant="secondary" disabled>
                Disabled secondary
              </Button>
            </div>
            <div className="flex items-center gap-4 flex-wrap">
              <Button variant="ghost">Cancel</Button>
              <Button variant="ghost">Dismiss</Button>
              <Button variant="ghost" disabled>
                Disabled ghost
              </Button>
            </div>
          </div>
        </Card>
      </Section>

      {/* ─── Pills ────────────────────────────────────────────────────── */}
      <Section
        title="Pills, badges, chips (PRD §16.6)"
        caption="Pill is the base; RecordingBadge and ProviderChip are named wrappers."
      >
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

      {/* ─── Cards ────────────────────────────────────────────────────── */}
      <Section title="Cards" caption="Three paddings; optional active border.">
        <div className="grid gap-4 md:grid-cols-3">
          <Card padding="sm">
            <h3 className="font-sans font-bold text-[16px] text-ink">
              Small card
            </h3>
            <p className="mt-1 text-[12px] text-mut">
              padding="sm" — compact rows.
            </p>
          </Card>
          <Card padding="md">
            <h3 className="font-sans font-bold text-[16px] text-ink">
              Medium card
            </h3>
            <p className="mt-1 text-[12px] text-mut">
              padding="md" (default) — most surfaces.
            </p>
          </Card>
          <Card padding="lg" active>
            <h3 className="font-sans font-bold text-[16px] text-ink">
              Active card
            </h3>
            <p className="mt-1 text-[12px] text-mut">
              padding="lg" + active — current step in onboarding.
            </p>
          </Card>
        </div>
      </Section>

      {/* ─── BrowserChrome ───────────────────────────────────────────── */}
      <Section
        title="BrowserChrome"
        caption="Fake-Safari wrapper for marketing mockups."
      >
        <BrowserChrome url="localhost:7878/welcome">
          <div className="p-10 flex items-center gap-4">
            <Logo size={32} />
            <div>
              <h3 className="font-serif text-[28px] text-ink leading-none">
                Welcome to yogurt.
              </h3>
              <p className="mt-2 text-[13px] text-mut">
                Two streams, one set of notes, zero bots in the call.
              </p>
            </div>
          </div>
        </BrowserChrome>
      </Section>

      {/* ─── Iconography demo (DESIGN-06) ────────────────────────────── */}
      <Section
        title="Iconography (DESIGN-06, Phase-1 strategy)"
        caption="Inline SVGs + Unicode glyphs. Full Lucide / Phosphor library selection deferred to Phase 7 per CONTEXT D-13."
      >
        <Card padding="lg">
          <div className="space-y-6">
            <div>
              <p className="font-mono text-[11px] text-mut uppercase tracking-wide mb-3">
                Inline SVG — brand mark at three sizes
              </p>
              <div className="flex items-end gap-6">
                <Logo size={19} ariaLabel="Yogurt small" />
                <Logo size={32} ariaLabel="Yogurt medium" />
                <Logo size={60} ariaLabel="Yogurt large" />
              </div>
            </div>
            <div>
              <p className="font-mono text-[11px] text-mut uppercase tracking-wide mb-3">
                Unicode glyphs in context
              </p>
              <div className="flex items-center gap-3 flex-wrap">
                <Button>+ New meeting</Button>
                <Button variant="secondary">⌘K Search</Button>
                <Button variant="ghost">Dismiss ↳</Button>
                <Pill tone="matcha">✓ Saved</Pill>
                <Pill tone="blue">✨ Enhance</Pill>
                <Pill>⌄ Re-enhance</Pill>
                <Pill tone="straw">⚙ Settings</Pill>
              </div>
              <p className="mt-3 text-[12px] text-mut">
                Phase 1 uses Unicode glyphs (⌘ ✓ + → ↳ ⌄ ✨ ⚙) inline with text
                to demonstrate the icon strategy. Phase 7 will select between
                Lucide and Phosphor for a full icon set.
              </p>
            </div>
          </div>
        </Card>
      </Section>

      {/* ─── Footer ──────────────────────────────────────────────────── */}
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
  children: ReactNode;
}) {
  return (
    <section>
      <h2 className="font-serif text-[30px] text-ink leading-tight">{title}</h2>
      <p className="mt-1 text-[13px] text-mut">{caption}</p>
      <div className="mt-5">{children}</div>
    </section>
  );
}
