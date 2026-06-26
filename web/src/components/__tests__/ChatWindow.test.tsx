import { describe, it, expect, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import type { ChatMessage } from "../../lib/api";
import { ChatWindow } from "../ChatWindow";

const sampleMessages: ChatMessage[] = [
  {
    id: "u1",
    meeting_id: "m1",
    role: "user",
    content: "hello",
    created_at: 1,
  },
  {
    id: "a1",
    meeting_id: "m1",
    role: "assistant",
    content: "hi there",
    created_at: 2,
  },
];

describe("ChatWindow", () => {
  it("renders all messages (user + assistant bubbles)", () => {
    render(
      <ChatWindow
        messages={sampleMessages}
        streamingId={null}
        onSend={() => {}}
        onCollapse={() => {}}
      />,
    );
    expect(screen.getByText("hello")).toBeInTheDocument();
    expect(screen.getByText("hi there")).toBeInTheDocument();
  });

  it("sends on Enter and clears the input", () => {
    const onSend = vi.fn();
    render(
      <ChatWindow
        messages={[]}
        streamingId={null}
        onSend={onSend}
        onCollapse={() => {}}
      />,
    );
    const input = screen.getByPlaceholderText(
      /ask this meeting/i,
    ) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "what was decided?" } });
    expect(input.value).toBe("what was decided?");
    fireEvent.keyDown(input, { key: "Enter" });
    expect(onSend).toHaveBeenCalledWith("what was decided?");
    expect(input.value).toBe("");
  });

  it("does NOT call onCollapse when the user clicks outside the window", () => {
    const onCollapse = vi.fn();
    const { container } = render(
      <div>
        <div data-testid="outside">outside region</div>
        <ChatWindow
          messages={sampleMessages}
          streamingId={null}
          onSend={() => {}}
          onCollapse={onCollapse}
        />
      </div>,
    );
    fireEvent.mouseDown(screen.getByTestId("outside"));
    fireEvent.mouseUp(screen.getByTestId("outside"));
    fireEvent.click(screen.getByTestId("outside"));
    expect(onCollapse).not.toHaveBeenCalled();
    // Sanity: the window itself is in the DOM.
    expect(
      container.querySelector('[aria-label="Ask the meeting chat"]'),
    ).not.toBeNull();
  });

  it("calls onCollapse exactly once when the collapse caret is clicked", () => {
    const onCollapse = vi.fn();
    render(
      <ChatWindow
        messages={sampleMessages}
        streamingId={null}
        onSend={() => {}}
        onCollapse={onCollapse}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: /collapse chat/i }));
    expect(onCollapse).toHaveBeenCalledTimes(1);
  });
});
