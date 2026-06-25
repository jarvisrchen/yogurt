import { useEditor, EditorContent } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { useEffect, useState } from "react";
import { fetchHealth, type HealthResponse } from "./lib/api";

export function App() {
  const [health, setHealth] = useState<HealthResponse | null>(null);
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
        // LO-03: surface server-unreachable inline rather than
        // silently console.error'ing and showing "loading…" forever.
        console.error(e);
        setHealthError(
          e instanceof Error ? e.message : "server unreachable — check terminal",
        );
      });
  }, []);

  return (
    <main className="max-w-2xl mx-auto p-10 space-y-6">
      <header className="space-y-1">
        <h1 className="text-3xl font-bold tracking-tight">yogurt</h1>
        <p className="text-sm text-neutral-500">
          phase 0 scaffold · server says:{" "}
          <code
            className={
              healthError
                ? "bg-red-100 text-red-800 px-2 py-0.5 rounded"
                : "bg-neutral-100 px-2 py-0.5 rounded"
            }
          >
            {healthError
              ? `unreachable — ${healthError}`
              : health
                ? `${health.service} ${health.status}`
                : "loading…"}
          </code>
        </p>
      </header>
      <section className="border border-neutral-300 rounded-lg p-4 bg-white">
        <EditorContent editor={editor} />
      </section>
    </main>
  );
}
