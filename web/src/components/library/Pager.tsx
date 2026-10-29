import { PAGE_SIZES, pageCount, rangeLabel } from "./pagination";

interface Props {
  page: number;
  size: number;
  total: number;
  onPage: (page: number) => void;
  onSize: (size: number) => void;
}

export const PAGER_BUTTON =
  "px-3 py-1.5 rounded-button border border-line text-[13px] font-medium text-ink hover:bg-paper disabled:opacity-40 disabled:hover:bg-transparent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue/40";

/** Hidden while every meeting fits on the smallest page, where no page or size choice changes anything. */
export function Pager({ page, size, total, onPage, onSize }: Props) {
  if (total <= PAGE_SIZES[0]) return null;
  const last = pageCount(size, total);
  return (
    <nav
      aria-label="Pagination"
      className="mt-8 flex flex-wrap items-center justify-between gap-3 text-[13px] text-mut"
    >
      <span data-testid="pager-range">{rangeLabel(page, size, total)}</span>
      <div className="flex items-center gap-2">
        <label className="flex items-center gap-2">
          <span className="whitespace-nowrap">Per page</span>
          <select
            aria-label="Meetings per page"
            className="rounded-button border border-line bg-paper px-2 py-1.5 text-[13px] text-ink focus:border-blue focus:outline-none"
            value={size}
            onChange={(e) => onSize(Number(e.target.value))}
          >
            {PAGE_SIZES.map((n) => (
              <option key={n} value={n}>
                {n}
              </option>
            ))}
          </select>
        </label>
        <button
          type="button"
          className={PAGER_BUTTON}
          disabled={page <= 1}
          onClick={() => onPage(page - 1)}
        >
          Previous
        </button>
        <button
          type="button"
          className={PAGER_BUTTON}
          disabled={page >= last}
          onClick={() => onPage(page + 1)}
        >
          Next
        </button>
      </div>
    </nav>
  );
}
