import type { Meeting } from "../../lib/api/meetings";
import { MeetingCard } from "./MeetingCard";
import { LABEL } from "../settings/labelClass";
import { groupMeetingsByDay } from "./dayGroups";

interface Props {
  meetings: ReadonlyArray<Meeting>;
  now?: Date;
  activeId?: string | null;
  selectionActive?: boolean;
  selectedIds?: ReadonlySet<string>;
  onToggleSelect?: (id: string, shiftKey: boolean) => void;
}

export function DateGroup({
  meetings,
  now = new Date(),
  activeId,
  selectionActive,
  selectedIds,
  onToggleSelect,
}: Props) {
  const groups = groupMeetingsByDay(meetings, now);
  return (
    <div className="flex flex-col gap-8">
      {groups.map((g) => (
        <section key={g.key}>
          <h2 className={`${LABEL} mb-2`}>
            {g.label}
          </h2>
          <ul className="flex flex-col gap-2">
            {g.meetings.map((m) => (
              <li key={m.id}>
                <MeetingCard
                  meeting={m}
                  activeId={activeId}
                  selectionActive={selectionActive}
                  selected={selectedIds?.has(m.id) ?? false}
                  onToggleSelect={onToggleSelect}
                />
              </li>
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}
