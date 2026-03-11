import { useState, useMemo } from 'react';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { Badge } from '@/components/ui/badge';
import { Alert, AlertDescription } from '@/components/ui/alert';
import {
  Copy,
  Check,
  Sparkles,
  ArrowRight,
  AlertCircle,
  CheckCircle,
} from 'lucide-react';
import { useDiscoveryStore, type AISuggestion } from '@/stores/discovery';
import type { TechnologyHint } from '@/api/discovery';

interface AIAssistantModalProps {
  open: boolean;
  onClose: () => void;
  serviceIndices: number[];
}

type Step = 'generate' | 'paste' | 'review';

interface ParsedSuggestion {
  process: string;
  technology: {
    id: string;
    display_name: string;
    icon: string;
    layer: string;
  };
  suggested_name: string;
  description: string;
  commands: {
    check?: string;
    start?: string;
    stop?: string;
  };
  confidence: 'high' | 'medium' | 'low';
}

export function AIAssistantModal({ open, onClose, serviceIndices }: AIAssistantModalProps) {
  const [step, setStep] = useState<Step>('generate');
  const [copied, setCopied] = useState(false);
  const [aiResponse, setAiResponse] = useState('');
  const [parseError, setParseError] = useState<string | null>(null);
  const [parsedSuggestions, setParsedSuggestions] = useState<ParsedSuggestion[]>([]);

  const correlationResult = useDiscoveryStore((s) => s.correlationResult);
  const setAISuggestion = useDiscoveryStore((s) => s.setAISuggestion);
  const setServiceTriageStatus = useDiscoveryStore((s) => s.setServiceTriageStatus);
  const updateServiceEdit = useDiscoveryStore((s) => s.updateServiceEdit);

  const services = useMemo(() => {
    return serviceIndices
      .map((i) => correlationResult?.services[i])
      .filter(Boolean);
  }, [serviceIndices, correlationResult]);

  // Generate the prompt
  const prompt = useMemo(() => {
    if (services.length === 0) return '';

    const processDescriptions = services.map((svc, idx) => {
      const s = svc!;
      return `${idx + 1}. Process: ${s.process_name}
   Host: ${s.hostname}
   Ports: ${s.ports.length > 0 ? s.ports.join(', ') : 'none'}
   Command suggestion source: ${s.command_suggestion?.source || 'unknown'}`;
    }).join('\n\n');

    return `Tu es un expert en infrastructure IT (banque, entreprise). Identifie ces processus non reconnus.

## Processus a identifier

${processDescriptions}

## Instructions

Pour chaque processus, determine:
1. La technologie (ElasticSearch, RabbitMQ, IBM MQ, Oracle, WebLogic, Control-M, etc.)
2. Un nom explicite pour le composant
3. Une courte description operationnelle
4. Les commandes check/start/stop appropriees

## Format de reponse (JSON strict)

Reponds UNIQUEMENT avec un tableau JSON, sans texte avant ou apres:

[
  {
    "process": "nom_du_process.exe",
    "technology": {
      "id": "elasticsearch",
      "display_name": "ElasticSearch",
      "icon": "elastic",
      "layer": "Database"
    },
    "suggested_name": "ElasticSearch@hostname",
    "description": "Moteur de recherche pour les logs applicatifs",
    "commands": {
      "check": "commande de verification",
      "start": "commande de demarrage",
      "stop": "commande d'arret"
    },
    "confidence": "high"
  }
]

Icones disponibles: elastic, mysql, postgresql, oracle, sqlserver, mongodb, redis, rabbitmq, kafka, ibmmq, tibco, weblogic, websphere, tomcat, nginx, apache, iis, controlm, autosys, docker, nodejs, dotnet

Layers: Database, Middleware, Application, Access Points, Scheduler, File Transfer, Security, Infrastructure`;
  }, [services]);

  const handleCopy = async () => {
    await navigator.clipboard.writeText(prompt);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const handlePaste = async () => {
    try {
      const text = await navigator.clipboard.readText();
      setAiResponse(text);
    } catch {
      // Fallback - user will paste manually
    }
  };

  const handleParseResponse = () => {
    setParseError(null);

    try {
      // Try to extract JSON from the response
      let jsonStr = aiResponse.trim();

      // Handle markdown code blocks
      const codeBlockMatch = jsonStr.match(/```(?:json)?\s*([\s\S]*?)```/);
      if (codeBlockMatch) {
        jsonStr = codeBlockMatch[1].trim();
      }

      // Handle if response starts/ends with extra text
      const arrayStart = jsonStr.indexOf('[');
      const arrayEnd = jsonStr.lastIndexOf(']');
      if (arrayStart !== -1 && arrayEnd !== -1) {
        jsonStr = jsonStr.slice(arrayStart, arrayEnd + 1);
      }

      const parsed = JSON.parse(jsonStr) as ParsedSuggestion[];

      if (!Array.isArray(parsed)) {
        throw new Error('Response is not an array');
      }

      // Validate structure
      parsed.forEach((item, idx) => {
        if (!item.process || !item.technology || !item.suggested_name) {
          throw new Error(`Item ${idx + 1} is missing required fields`);
        }
      });

      setParsedSuggestions(parsed);
      setStep('review');
    } catch (e) {
      setParseError(
        `Failed to parse AI response: ${e instanceof Error ? e.message : 'Invalid JSON'}.
        Make sure the response is valid JSON array.`
      );
    }
  };

  const handleApplySuggestions = () => {
    // Match suggestions to services and apply
    parsedSuggestions.forEach((suggestion) => {
      const serviceIdx = serviceIndices.find((i) => {
        const svc = correlationResult?.services[i];
        return svc?.process_name.toLowerCase() === suggestion.process.toLowerCase();
      });

      if (serviceIdx !== undefined) {
        // Store the AI suggestion
        const aiSuggestion: AISuggestion = {
          technology: suggestion.technology as TechnologyHint,
          suggestedName: suggestion.suggested_name,
          description: suggestion.description,
          commands: suggestion.commands,
          confidence: suggestion.confidence,
        };
        setAISuggestion(serviceIdx, aiSuggestion);

        // Update service edits with the suggested values
        updateServiceEdit(serviceIdx, {
          name: suggestion.suggested_name,
          componentType: suggestion.technology.layer.toLowerCase(),
          checkCmd: suggestion.commands.check,
          startCmd: suggestion.commands.start,
          stopCmd: suggestion.commands.stop,
        });

        // Auto-include the service
        setServiceTriageStatus(serviceIdx, 'include');
      }
    });

    // Reset and close
    setStep('generate');
    setAiResponse('');
    setParsedSuggestions([]);
    onClose();
  };

  const handleClose = () => {
    setStep('generate');
    setAiResponse('');
    setParseError(null);
    setParsedSuggestions([]);
    onClose();
  };

  return (
    <Dialog open={open} onOpenChange={handleClose}>
      <DialogContent className="max-w-2xl max-h-[85vh] overflow-hidden flex flex-col">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Sparkles className="h-5 w-5 text-violet-500" />
            AI Assistant
          </DialogTitle>
          <DialogDescription>
            {step === 'generate' && 'Copy this prompt and paste it into Claude or ChatGPT.'}
            {step === 'paste' && 'Paste the AI response here.'}
            {step === 'review' && 'Review the suggestions before applying.'}
          </DialogDescription>
        </DialogHeader>

        <div className="flex-1 overflow-auto py-4">
          {step === 'generate' && (
            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <Badge variant="secondary">
                  {serviceIndices.length} process{serviceIndices.length > 1 ? 'es' : ''} to identify
                </Badge>
                <Button
                  variant="outline"
                  size="sm"
                  className="gap-2"
                  onClick={handleCopy}
                >
                  {copied ? (
                    <>
                      <Check className="h-4 w-4 text-emerald-500" />
                      Copied!
                    </>
                  ) : (
                    <>
                      <Copy className="h-4 w-4" />
                      Copy Prompt
                    </>
                  )}
                </Button>
              </div>
              <Textarea
                value={prompt}
                readOnly
                className="h-[300px] font-mono text-xs"
              />
            </div>
          )}

          {step === 'paste' && (
            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <span className="text-sm text-muted-foreground">
                  Paste the JSON response from the AI
                </span>
                <Button
                  variant="outline"
                  size="sm"
                  className="gap-2"
                  onClick={handlePaste}
                >
                  <Copy className="h-4 w-4" />
                  Paste from clipboard
                </Button>
              </div>
              <Textarea
                value={aiResponse}
                onChange={(e) => setAiResponse(e.target.value)}
                placeholder="Paste the AI response here..."
                className="h-[300px] font-mono text-xs"
              />
              {parseError && (
                <Alert variant="destructive">
                  <AlertCircle className="h-4 w-4" />
                  <AlertDescription className="text-xs">
                    {parseError}
                  </AlertDescription>
                </Alert>
              )}
            </div>
          )}

          {step === 'review' && (
            <div className="space-y-4">
              <Alert>
                <CheckCircle className="h-4 w-4 text-emerald-500" />
                <AlertDescription>
                  Found {parsedSuggestions.length} suggestion{parsedSuggestions.length > 1 ? 's' : ''}.
                  Review and click Apply to use them.
                </AlertDescription>
              </Alert>
              <div className="space-y-3 max-h-[300px] overflow-auto">
                {parsedSuggestions.map((s, idx) => (
                  <div
                    key={idx}
                    className="p-3 rounded-lg border bg-card"
                  >
                    <div className="flex items-center justify-between mb-2">
                      <span className="font-medium">{s.suggested_name}</span>
                      <Badge
                        variant={
                          s.confidence === 'high' ? 'default' :
                          s.confidence === 'medium' ? 'secondary' : 'outline'
                        }
                        className="text-[10px]"
                      >
                        {s.confidence}
                      </Badge>
                    </div>
                    <div className="text-xs text-muted-foreground mb-2">
                      {s.description}
                    </div>
                    <div className="flex items-center gap-2">
                      <Badge variant="secondary" className="text-[10px]">
                        {s.technology.display_name}
                      </Badge>
                      <Badge variant="outline" className="text-[10px]">
                        {s.technology.layer}
                      </Badge>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>

        <DialogFooter>
          {step === 'generate' && (
            <>
              <Button variant="outline" onClick={handleClose}>
                Cancel
              </Button>
              <Button onClick={() => setStep('paste')} className="gap-2">
                I've copied the prompt
                <ArrowRight className="h-4 w-4" />
              </Button>
            </>
          )}
          {step === 'paste' && (
            <>
              <Button variant="outline" onClick={() => setStep('generate')}>
                Back
              </Button>
              <Button
                onClick={handleParseResponse}
                disabled={!aiResponse.trim()}
                className="gap-2"
              >
                Parse Response
                <ArrowRight className="h-4 w-4" />
              </Button>
            </>
          )}
          {step === 'review' && (
            <>
              <Button variant="outline" onClick={() => setStep('paste')}>
                Back
              </Button>
              <Button
                onClick={handleApplySuggestions}
                className="gap-2"
              >
                <CheckCircle className="h-4 w-4" />
                Apply {parsedSuggestions.length} Suggestion{parsedSuggestions.length > 1 ? 's' : ''}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
