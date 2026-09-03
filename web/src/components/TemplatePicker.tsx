// TemplatePicker - which note format the next Re-enhance uses.
//
// A native <select> next to the Re-enhance button: "Auto" lets the model
// pick the best fit (the default flow), and after an enhance the value
// shows the format that was actually used, so the next Re-enhance keeps
// it unless the user changes it back to Auto. Native so it costs no
// dependency and inherits keyboard and screen-reader behavior for free.

import { AUTO_TEMPLATE, useTemplates } from "../lib/api/templates";

export interface TemplatePickerProps {
  /** `AUTO_TEMPLATE` or a template id. */
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
}

export function TemplatePicker({ value, onChange, disabled }: TemplatePickerProps) {
  const templates = useTemplates();
  const list = templates.data ?? [];
  // A stored id the list does not know (an older build's template, or the
  // list has not loaded yet) still needs to render as the selected value
  // rather than snapping the control back to Auto.
  const known = list.some((t) => t.id === value);
  const current = list.find((t) => t.id === value);
  return (
    <select
      aria-label="Note format"
      data-testid="template-picker"
      value={value}
      disabled={disabled}
      title={current?.when ?? "Let the model pick the format that fits the meeting"}
      onChange={(e) => onChange(e.target.value)}
      className="h-[34px] rounded-[9px] border border-line bg-paper px-2.5 text-[13px] font-semibold text-ink shadow-[0_1px_2px_rgba(40,30,15,0.06)] transition-colors hover:border-mut focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue disabled:cursor-wait disabled:opacity-70"
    >
      <option value={AUTO_TEMPLATE}>Auto</option>
      {list.map((t) => (
        <option key={t.id} value={t.id} title={t.when}>
          {t.name}
        </option>
      ))}
      {!known && value !== AUTO_TEMPLATE && <option value={value}>{value}</option>}
    </select>
  );
}
