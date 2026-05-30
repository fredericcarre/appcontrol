import { useMutation } from '@tanstack/react-query';
import client from './client';

export interface AiChatRequest {
  message: string;
}

export interface AiChatResponse {
  answer: string;
  /** Where inference ran: "local" (sovereign) or "frontier". Sovereignty transparency. */
  routed_to: string;
  model: string;
  /** Data-sensitivity classification of the prompt: public | internal | sensitive | secret. */
  sensitivity: string;
}

/**
 * Ask the read-only operations copilot a question.
 *
 * The backend records every turn in the append-only `ai_decisions` table and
 * routes inference through the sovereign router (local for sensitive data,
 * frontier only for redacted/non-sensitive context).
 */
export function useAiChat() {
  return useMutation({
    mutationFn: async (body: AiChatRequest) => {
      const { data } = await client.post<AiChatResponse>('/ai/chat', body);
      return data;
    },
  });
}
