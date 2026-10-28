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

  it("shows a muted placeholder for a finished-but-empty assistant reply", () => {
    render(
      <ChatWindow
        messages={[
          {
            id: "a-empty",
            meeting_id: "m1",
            role: "assistant",
            content: "\n",
            created_at: 3,
          },
        ]}
        streamingId={null}
        onSend={() => {}}
        onCollapse={() => {}}
      />,
    );
    expect(screen.getByText("No response.")).toBeInTheDocument();
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

  it("shows suggested-question chips in the empty state and sends on click", () => {
    const onSend = vi.fn();
    render(
      <ChatWindow
        messages={[]}
        streamingId={null}
        onSend={onSend}
        onCollapse={() => {}}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Catch me up" }));
    expect(onSend).toHaveBeenCalledWith("Catch me up");
  });

  it("does not show suggested-question chips once there are messages", () => {
    render(
      <ChatWindow
        messages={sampleMessages}
        streamingId={null}
        onSend={() => {}}
        onCollapse={() => {}}
      />,
    );
    expect(
      screen.queryByRole("button", { name: "Catch me up" }),
    ).not.toBeInTheDocument();
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

  it("autofocuses the input on mount", () => {
    render(
      <ChatWindow
        messages={[]}
        streamingId={null}
        onSend={() => {}}
        onCollapse={() => {}}
      />,
    );
    expect(screen.getByPlaceholderText(/ask this meeting/i)).toHaveFocus();
  });

  it("calls onCollapse when Escape is pressed", () => {
    const onCollapse = vi.fn();
    render(
      <ChatWindow
        messages={[]}
        streamingId={null}
        onSend={() => {}}
        onCollapse={onCollapse}
      />,
    );
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onCollapse).toHaveBeenCalledTimes(1);
  });

  it("collapses on Escape in its own input but not in another field", () => {
    const onCollapse = vi.fn();
    render(
      <>
        <input aria-label="Search" />
        <ChatWindow messages={[]} streamingId={null} onSend={() => {}} onCollapse={onCollapse} />
      </>,
    );
    fireEvent.keyDown(screen.getByLabelText("Search"), { key: "Escape" });
    expect(onCollapse).not.toHaveBeenCalled();
    fireEvent.keyDown(screen.getByPlaceholderText(/ask this meeting/i), { key: "Escape" });
    expect(onCollapse).toHaveBeenCalledTimes(1);
  });

  it("does not send on the Enter that commits an IME composition", () => {
    const onSend = vi.fn();
    render(<ChatWindow messages={[]} streamingId={null} onSend={onSend} onCollapse={() => {}} />);
    const input = screen.getByPlaceholderText(/ask this meeting/i);
    fireEvent.change(input, { target: { value: "会議" } });
    fireEvent.keyDown(input, { key: "Enter", isComposing: true });
    expect(onSend).not.toHaveBeenCalled();
    fireEvent.keyDown(input, { key: "Enter" });
    expect(onSend).toHaveBeenCalledWith("会議");
  });

  it("keeps a question typed while an answer streams until it finishes", () => {
    const onSend = vi.fn();
    const { rerender } = render(<ChatWindow messages={sampleMessages} streamingId="a1" onSend={onSend} onCollapse={() => {}} />);
    const input = screen.getByPlaceholderText(/ask this meeting/i);
    fireEvent.change(input, { target: { value: "and then?" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(onSend).not.toHaveBeenCalled();
    expect(input).toHaveValue("and then?");
    rerender(<ChatWindow messages={sampleMessages} streamingId={null} onSend={onSend} onCollapse={() => {}} />);
    fireEvent.keyDown(input, { key: "Enter" });
    expect(onSend).toHaveBeenCalledWith("and then?");
  });

  it("renders assistant bullets and bold text for a markdown-ish reply", () => {
    render(
      <ChatWindow
        messages={[
          {
            id: "a1",
            meeting_id: "m1",
            role: "assistant",
            content: "Decided:\n- **ship** it\n- follow up later",
            created_at: 1,
          },
        ]}
        streamingId={null}
        onSend={() => {}}
        onCollapse={() => {}}
      />,
    );
    expect(screen.getByText("ship")).toBeInTheDocument();
    expect(screen.getByText("ship").tagName).toBe("STRONG");
    expect(screen.getByText("follow up later")).toBeInTheDocument();
  });
});
