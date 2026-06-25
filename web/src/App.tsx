import { useEditor, EditorContent } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { useEffect, useState } from "react";
import { fetchHealth, type HealthResponse } from "./lib/api";

export function App() {
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const editor = useEditor({
    extensions: [StarterKit],
    content: "<p>Type something — TipTap is working.</p>",
  });

  useEffect(() => {
    fetchHealth()
      .then(setHealth)
      .catch((e) => console.error(e));
  }, []);

  return (
    <main className="max-w-2xl mx-auto p-10 space-y-6">
      <header className="space-y-1">
        <h1 className="text-3xl font-bold tracking-tight">yogurt</h1>
        <p className="text-sm text-neutral-500">
          phase 0 scaffold · server says:{" "}
          <code className="bg-neutral-100 px-2 py-0.5 rounded">
            {health ? `${health.service} ${health.status}` : "loading…"}
          </code>
        </p>
      </header>
      <section className="border border-neutral-300 rounded-lg p-4 bg-white">
        <EditorContent editor={editor} />
      </section>
    </main>
  );
}
