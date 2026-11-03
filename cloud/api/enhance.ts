import { handleEnhance } from "./_enhance.js";

// The only environment the function reads. Vercel also hands it the database URL the build uses, which it must never touch.
const { SUPABASE_URL, SUPABASE_PUBLISHABLE_KEY, YOGURT_LLM_BASE_URL, YOGURT_LLM_MODEL, YOGURT_LLM_API_KEY } = process.env;

// An hour of transcript can stream for over a minute.
export const config = { maxDuration: 300 };

export const POST = (req: Request) =>
  handleEnhance(req, { SUPABASE_URL, SUPABASE_PUBLISHABLE_KEY, YOGURT_LLM_BASE_URL, YOGURT_LLM_MODEL, YOGURT_LLM_API_KEY });
