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
  // LO-03 (Phase 0): surface server-unreachable errors inline rather than
  // silently console.error'ing and showing "loading…" forever.
  const [healthError, setHealthError] = useState<string | null>(null);
  const editor = useEditor({
    extensions: [StarterKit],
    content: "<p>Type something — TipTap is working.</p>",
  });

  useEffect(() => {
    fetchHealth()
      .then((h) => {
        setHealth(h);
        setHealthError(null);
      })
      .catch((e) => {
        console.error(e);
        setHealthError(
          e instanceof Error ? e.message : "server unreachable — check terminal",
        );
      });
  }, []);

  return (
    <main className="mx-auto max-w-2xl px-10 py-12 space-y-6">
      <header className="flex items-center gap-3">
        {/* Decorative — adjacent <h1>yogurt</h1> already names the brand. */}
        <Logo size={44} />
        <div>
          <h1 className="font-serif text-[44px] leading-none text-ink">
            yogurt
          </h1>
          <p className="mt-1 text-[13px] text-mut">
            phase 1 design system ·{" "}
            <Link
              to="/style-guide"
              className="text-blue underline-offset-2 hover:underline rounded-chip focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue/40 focus-visible:ring-offset-2 focus-visible:ring-offset-paper"
            >
              /style-guide →
            </Link>
          </p>
        </div>
      </header>

      <div>
        <Pill tone={healthError ? "straw" : "matcha"}>
          server:{" "}
          <code className="font-mono">
            {healthError
              ? `unreachable — ${healthError}`
              : health
                ? `${health.service} ${health.status}`
                : "loading…"}
          </code>
        </Pill>
      </div>

      <Card padding="md">
        <EditorContent editor={editor} />
      </Card>
    </main>
  );
}
