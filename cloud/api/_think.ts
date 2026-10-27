// A port of crates/yogurt-llm/src/thinking.rs: drops leading <think>...</think> reasoning from a streamed answer.
const OPEN = "<think>";
const CLOSE = "</think>";

export class ThinkStripper {
  private state: "start" | "thinking" | "visible" = "start";
  private pending = "";

  push(delta: string): string {
    if (this.state === "visible") return delta;
    this.pending += delta;
    for (;;) {
      if (this.state === "start") {
        const trimmed = this.pending.trimStart();
        if (OPEN.startsWith(trimmed)) return "";
        if (trimmed.startsWith(OPEN)) {
          this.pending = trimmed.slice(OPEN.length);
          this.state = "thinking";
          continue;
        }
        this.state = "visible";
        return this.take();
      }
      if (this.state === "thinking") {
        const end = this.pending.indexOf(CLOSE);
        if (end >= 0) {
          this.pending = this.pending.slice(end + CLOSE.length).trimStart();
          this.state = "start";
          continue;
        }
        this.pending = this.pending.slice(this.pending.length - partialSuffix(this.pending, CLOSE));
        return "";
      }
      return this.take();
    }
  }

  /** Keeps unresolved visible text and drops an unterminated reasoning block. */
  flush(): string {
    if (this.state === "thinking") {
      this.pending = "";
      return "";
    }
    return this.take();
  }

  private take() {
    const out = this.pending;
    this.pending = "";
    return out;
  }
}

function partialSuffix(s: string, tag: string) {
  for (let i = 0; i < s.length; i++) if (tag.startsWith(s.slice(i))) return s.length - i;
  return 0;
}
