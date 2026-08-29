/**
 * `MeetingLabels` — chips for a meeting's labels + a "+ Label" ghost
 * button that opens `LabelPicker`. Mounted in the live meeting header
 * (`routes/Meeting.tsx`) and the post-meeting header (`routes/MeetingPost.tsx`).
 */

import { useState } from "react";
import { Tag } from "lucide-react";
import { useMeeting, useSetMeetingLabels } from "../../lib/api/meetings";
import type { Label } from "../../lib/api/labels";
import { LabelChip } from "./LabelChip";
import { LabelPicker } from "./LabelPicker";

interface Props {
  meetingId: string;
  labels?: Label[];
  compact?: boolean;
}

export function MeetingLabels({ meetingId, labels, compact }: Props) {
  const [open, setOpen] = useState(false);
  const meetingQuery = useMeeting(labels ? undefined : meetingId);
  const setLabels = useSetMeetingLabels();
  const resolved = labels ?? meetingQuery.data?.labels ?? [];

  function removeLabel(id: string) {
    setLabels.mutate({
      id: meetingId,
      label_ids: resolved.filter((l) => l.id !== id).map((l) => l.id),
    });
  }

  return (
    <div className="relative inline-flex items-center flex-wrap gap-1.5">
      {resolved.map((l) => (
        <LabelChip
          key={l.id}
          label={l}
          size={compact ? "sm" : "md"}
          onRemove={() => removeLabel(l.id)}
        />
      ))}
      <button
        type="button"
        // See MeetingCardActions' Tag button for why mousedown is also
        // stopped here (LabelPicker's outside-click listener fires on
        // mousedown, before this onClick runs).
        onMouseDown={(e) => e.stopPropagation()}
        onClick={() => setOpen((o) => !o)}
        className="inline-flex items-center gap-1 text-mut hover:text-ink text-[12px] font-mono"
      >
        <Tag size={12} aria-hidden />
        Label
      </button>
      <LabelPicker
        meetingId={meetingId}
        selected={resolved}
        open={open}
        onClose={() => setOpen(false)}
      />
    </div>
  );
}
