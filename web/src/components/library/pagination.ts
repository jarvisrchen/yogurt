export const PAGE_SIZES = [10, 25, 50, 100] as const;
export const DEFAULT_PAGE_SIZE = 25;

const SIZE_KEY = "yogurt.library.pageSize";

export function parsePage(raw: string | null): number {
  const n = Number(raw);
  return Number.isInteger(n) && n >= 1 ? n : 1;
}

function parseSize(raw: string | null): number | null {
  const n = Number(raw);
  return PAGE_SIZES.includes(n as (typeof PAGE_SIZES)[number]) ? n : null;
}

/** The URL wins so a shared or restored link keeps its size, then the last choice, then the default. */
export function resolvePageSize(raw: string | null): number {
  const fromUrl = parseSize(raw);
  if (fromUrl) return fromUrl;
  try {
    return parseSize(localStorage.getItem(SIZE_KEY)) ?? DEFAULT_PAGE_SIZE;
  } catch {
    return DEFAULT_PAGE_SIZE;
  }
}

export function rememberPageSize(size: number): void {
  try {
    localStorage.setItem(SIZE_KEY, String(size));
  } catch {
    // Private windows and blocked storage just forget the choice.
  }
}

export const formatCount = (n: number) => n.toLocaleString("en-US");

/** "26–50 of 1,412". */
export function rangeLabel(page: number, size: number, total: number): string {
  if (total === 0) return "0 of 0";
  const first = (page - 1) * size + 1;
  const last = Math.min(page * size, total);
  return `${formatCount(first)}–${formatCount(last)} of ${formatCount(total)}`;
}

export const pageCount = (size: number, total: number) =>
  Math.max(1, Math.ceil(total / size));
