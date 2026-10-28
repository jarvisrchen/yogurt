// M/D/YY in local time, the form the server's default titles and date search use.
export function shortDate(unixMs: number): string {
  return new Date(unixMs).toLocaleDateString("en-US", {
    month: "numeric",
    day: "numeric",
    year: "2-digit",
  });
}

// The name and the M/D/YY date of a title that ends with its own start date, as default titles do.
export function splitTitle(title: string, startedAt?: number): { name: string; date: string | null } {
  if (startedAt == null) return { name: title, date: null };
  const date = shortDate(startedAt);
  const name = title.endsWith(` ${date}`) ? title.slice(0, -date.length - 1) : title;
  return { name, date };
}
