import { handleChat } from "./_chat.js";

// The only environment the function reads. Vercel also hands it the database URL the build uses, which it must never touch.
const { SUPABASE_URL, SUPABASE_PUBLISHABLE_KEY, YOGURT_LLM_BASE_URL, YOGURT_LLM_MODEL, YOGURT_LLM_API_KEY } = process.env;

// A short answer takes seconds, and this bounds a provider that stalls mid-stream.
export const config = { maxDuration: 60 };

export const POST = (req: Request) =>
  handleChat(req, { SUPABASE_URL, SUPABASE_PUBLISHABLE_KEY, YOGURT_LLM_BASE_URL, YOGURT_LLM_MODEL, YOGURT_LLM_API_KEY });
