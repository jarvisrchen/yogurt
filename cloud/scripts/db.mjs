/** Connection settings for the build's migration step. Supabase's pooler and direct hosts use a CA that Node does not trust, so TLS is on without verification, except on a local database. */
export function dbConfig(url) {
  const u = new URL(url);
  for (const k of [...u.searchParams.keys()]) if (k.toLowerCase().startsWith("ssl")) u.searchParams.delete(k);
  const local = u.hostname === "localhost" || u.hostname === "127.0.0.1";
  return { connectionString: u.toString(), ssl: local ? false : { rejectUnauthorized: false } };
}
