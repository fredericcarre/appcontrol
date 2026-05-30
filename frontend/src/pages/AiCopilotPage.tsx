import { useRef, useState } from 'react';
import { Bot, Send, ShieldCheck, Cloud, User, Sparkles } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';
import { useAiChat, type AiChatResponse } from '@/api/ai';

interface ChatMessage {
  role: 'user' | 'assistant';
  content: string;
  meta?: Pick<AiChatResponse, 'routed_to' | 'model' | 'sensitivity'>;
}

const EXAMPLE_PROMPTS = [
  'Which applications are currently degraded, and why?',
  'Summarize the dependencies of the payment platform.',
  'What changed in the last hour across my systems?',
];

export default function AiCopilotPage() {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [draft, setDraft] = useState('');
  const chat = useAiChat();
  const scrollRef = useRef<HTMLDivElement>(null);

  const scrollToBottom = () => {
    requestAnimationFrame(() => {
      scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: 'smooth' });
    });
  };

  const send = (text: string) => {
    const message = text.trim();
    if (!message || chat.isPending) return;
    setMessages((prev) => [...prev, { role: 'user', content: message }]);
    setDraft('');
    scrollToBottom();
    chat.mutate(
      { message },
      {
        onSuccess: (data) => {
          setMessages((prev) => [
            ...prev,
            {
              role: 'assistant',
              content: data.answer,
              meta: { routed_to: data.routed_to, model: data.model, sensitivity: data.sensitivity },
            },
          ]);
          scrollToBottom();
        },
      },
    );
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      send(draft);
    }
  };

  return (
    <div className="flex flex-col h-[calc(100vh-3.5rem)]">
      {/* Header */}
      <div className="flex items-center gap-3 px-6 py-4 border-b border-border">
        <div className="rounded-lg bg-primary/10 p-2">
          <Bot className="h-5 w-5 text-primary" />
        </div>
        <div className="flex-1">
          <h1 className="text-lg font-semibold leading-tight">Operations Copilot</h1>
          <p className="text-xs text-muted-foreground">
            Read-only — it explains and recommends. Any action goes through an approved AppControl
            operation.
          </p>
        </div>
        <Badge variant="outline" className="gap-1">
          <ShieldCheck className="h-3 w-3" /> Sovereign routing
        </Badge>
      </div>

      {/* Messages */}
      <div ref={scrollRef} className="flex-1 overflow-y-auto px-6 py-6 space-y-6">
        {messages.length === 0 && (
          <div className="max-w-2xl mx-auto text-center space-y-6 pt-10">
            <div className="inline-flex rounded-full bg-primary/10 p-4">
              <Sparkles className="h-7 w-7 text-primary" />
            </div>
            <div>
              <h2 className="text-base font-medium">Ask about the state of your systems</h2>
              <p className="text-sm text-muted-foreground mt-1">
                Sensitive context stays on a sovereign local model; only redacted, non-sensitive
                context can reach a frontier model.
              </p>
            </div>
            <div className="grid gap-2 sm:grid-cols-1 text-left">
              {EXAMPLE_PROMPTS.map((p) => (
                <button
                  key={p}
                  onClick={() => send(p)}
                  className="rounded-lg border border-border px-4 py-3 text-sm hover:bg-accent transition-colors"
                >
                  {p}
                </button>
              ))}
            </div>
          </div>
        )}

        {messages.map((m, i) => (
          <MessageBubble key={i} message={m} />
        ))}

        {chat.isPending && (
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <Bot className="h-4 w-4 animate-pulse" />
            Thinking…
          </div>
        )}

        {chat.isError && (
          <div className="rounded-lg border border-destructive/40 bg-destructive/10 px-4 py-3 text-sm text-destructive">
            The copilot is unavailable right now. It may be disabled (kill-switch) or no model is
            configured.
          </div>
        )}
      </div>

      {/* Composer */}
      <div className="border-t border-border px-6 py-4">
        <div className="flex items-end gap-2 max-w-3xl mx-auto">
          <Textarea
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={onKeyDown}
            placeholder="Ask the operations copilot…  (Enter to send, Shift+Enter for a new line)"
            rows={2}
            className="resize-none"
            aria-label="Message the operations copilot"
          />
          <Button
            onClick={() => send(draft)}
            disabled={!draft.trim() || chat.isPending}
            aria-label="Send message"
          >
            <Send className="h-4 w-4" />
          </Button>
        </div>
      </div>
    </div>
  );
}

function MessageBubble({ message }: { message: ChatMessage }) {
  const isUser = message.role === 'user';
  return (
    <div className={cn('flex gap-3', isUser ? 'justify-end' : 'justify-start')}>
      {!isUser && (
        <div className="rounded-full bg-primary/10 p-1.5 h-7 w-7 shrink-0">
          <Bot className="h-4 w-4 text-primary" />
        </div>
      )}
      <div className={cn('max-w-2xl space-y-1.5', isUser && 'items-end flex flex-col')}>
        <div
          className={cn(
            'rounded-lg px-4 py-2.5 text-sm whitespace-pre-wrap',
            isUser ? 'bg-primary text-primary-foreground' : 'bg-muted',
          )}
        >
          {message.content}
        </div>
        {message.meta && <RoutingFooter meta={message.meta} />}
      </div>
      {isUser && (
        <div className="rounded-full bg-muted p-1.5 h-7 w-7 shrink-0">
          <User className="h-4 w-4" />
        </div>
      )}
    </div>
  );
}

function RoutingFooter({ meta }: { meta: NonNullable<ChatMessage['meta']> }) {
  const isLocal = meta.routed_to === 'local';
  return (
    <div className="flex flex-wrap items-center gap-1.5 text-[11px] text-muted-foreground">
      <Badge variant="outline" className="gap-1 font-normal">
        {isLocal ? <ShieldCheck className="h-3 w-3" /> : <Cloud className="h-3 w-3" />}
        {isLocal ? 'Local / sovereign' : 'Frontier (redacted)'}
      </Badge>
      <span>·</span>
      <span>model: {meta.model}</span>
      <span>·</span>
      <span>sensitivity: {meta.sensitivity}</span>
    </div>
  );
}
