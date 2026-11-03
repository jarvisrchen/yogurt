import { handleSttToken } from "./_stt-token.js";

// The only environment the function reads. The Deepgram key never leaves it.
const { SUPABASE_URL, SUPABASE_PUBLISHABLE_KEY, YOGURT_DEEPGRAM_API_KEY, YOGURT_DEEPGRAM_URL } = process.env;

export const POST = (req: Request) => handleSttToken(req, { SUPABASE_URL, SUPABASE_PUBLISHABLE_KEY, YOGURT_DEEPGRAM_API_KEY, YOGURT_DEEPGRAM_URL });
