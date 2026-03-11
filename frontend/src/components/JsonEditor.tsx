import { useEffect, useRef, useCallback, useMemo } from 'react';
import { AlertCircle, Check } from 'lucide-react';

// ═══════════════════════════════════════════════════════════════════════════
// JSON Validation
// ═══════════════════════════════════════════════════════════════════════════

export interface JsonError {
  message: string;
  line: number;
  column: number;
  position: number;
}

export function validateJson(content: string): JsonError | null {
  if (!content.trim()) return null;

  try {
    JSON.parse(content);
    return null;
  } catch (e) {
    if (e instanceof SyntaxError) {
      // Parse the error message to extract position
      // Format: "... at position 123" or "... at line X column Y"
      const msg = e.message;

      // Try to extract position from error message
      const posMatch = msg.match(/position\s+(\d+)/i);
      const position = posMatch ? parseInt(posMatch[1], 10) : 0;

      // Calculate line and column from position
      const { line, column } = getLineColumn(content, position);

      // Clean up error message
      let cleanMessage = msg
        .replace(/^JSON\.parse:\s*/i, '')
        .replace(/\s*at position \d+/i, '')
        .replace(/\s*in JSON\s*/i, '');

      // Add helpful context for common errors
      if (msg.includes('Unexpected token')) {
        const tokenMatch = msg.match(/Unexpected token\s*['"]?(.+?)['"]?(?:\s|,|$)/i);
        if (tokenMatch) {
          cleanMessage = `Unexpected token: ${tokenMatch[1]}`;
        }
      }

      // Check for trailing comma (common error)
      const beforePos = content.substring(0, position).trim();
      if (beforePos.endsWith(',')) {
        cleanMessage = 'Trailing comma not allowed';
      }

      return {
        message: cleanMessage,
        line,
        column,
        position,
      };
    }
    return {
      message: 'Invalid JSON',
      line: 1,
      column: 1,
      position: 0,
    };
  }
}

function getLineColumn(content: string, position: number): { line: number; column: number } {
  const lines = content.substring(0, position).split('\n');
  return {
    line: lines.length,
    column: lines[lines.length - 1].length + 1,
  };
}

// ═══════════════════════════════════════════════════════════════════════════
// Syntax Highlighting
// ═══════════════════════════════════════════════════════════════════════════

function highlightJson(code: string): string {
  // Escape HTML
  let html = code
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');

  // String values (handles escaped quotes)
  html = html.replace(
    /"(?:[^"\\]|\\.)*"/g,
    (match) => {
      // Check if it's a key (followed by :) or a value
      return `<span class="json-string">${match}</span>`;
    }
  );

  // Numbers
  html = html.replace(
    /\b(-?\d+\.?\d*(?:[eE][+-]?\d+)?)\b/g,
    '<span class="json-number">$1</span>'
  );

  // Booleans and null
  html = html.replace(
    /\b(true|false|null)\b/g,
    '<span class="json-keyword">$1</span>'
  );

  // Keys (strings before :)
  html = html.replace(
    /<span class="json-string">("(?:[^"\\]|\\.)*")<\/span>\s*:/g,
    '<span class="json-key">$1</span>:'
  );

  return html;
}

// ═══════════════════════════════════════════════════════════════════════════
// JsonEditor Component
// ═══════════════════════════════════════════════════════════════════════════

interface JsonEditorProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  height?: string;
  readOnly?: boolean;
  onValidationChange?: (error: JsonError | null) => void;
}

export function JsonEditor({
  value,
  onChange,
  placeholder = '{\n  \n}',
  height = '400px',
  readOnly = false,
  onValidationChange,
}: JsonEditorProps) {
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const highlightRef = useRef<HTMLDivElement>(null);
  const lineNumbersRef = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const prevErrorRef = useRef<JsonError | null>(null);

  // Compute error synchronously via useMemo
  const error = useMemo(() => validateJson(value), [value]);

  // Notify parent of validation changes (only when error actually changes)
  useEffect(() => {
    const prevError = prevErrorRef.current;
    const errorChanged =
      (prevError === null && error !== null) ||
      (prevError !== null && error === null) ||
      (prevError !== null && error !== null && prevError.message !== error.message);

    if (errorChanged) {
      prevErrorRef.current = error;
      onValidationChange?.(error);
    }
  }, [error, onValidationChange]);

  // Line numbers
  const lines = useMemo(() => {
    const count = value ? value.split('\n').length : 1;
    return Array.from({ length: Math.max(count, 1) }, (_, i) => i + 1);
  }, [value]);

  // Sync scroll between textarea and highlight layer
  const handleScroll = useCallback(() => {
    if (textareaRef.current && highlightRef.current && lineNumbersRef.current) {
      highlightRef.current.scrollTop = textareaRef.current.scrollTop;
      highlightRef.current.scrollLeft = textareaRef.current.scrollLeft;
      lineNumbersRef.current.scrollTop = textareaRef.current.scrollTop;
    }
  }, []);

  // Highlighted HTML with error line marking
  const highlightedHtml = useMemo(() => {
    if (!value) return '';

    const lineArray = value.split('\n');
    return lineArray
      .map((line, idx) => {
        const lineNum = idx + 1;
        const isErrorLine = error && error.line === lineNum;
        const highlighted = highlightJson(line);

        if (isErrorLine) {
          return `<div class="json-line json-error-line">${highlighted || ' '}</div>`;
        }
        return `<div class="json-line">${highlighted || ' '}</div>`;
      })
      .join('');
  }, [value, error]);

  // Handle tab key for indentation
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === 'Tab') {
        e.preventDefault();
        const textarea = e.currentTarget;
        const start = textarea.selectionStart;
        const end = textarea.selectionEnd;

        // Insert 2 spaces
        const newValue = value.substring(0, start) + '  ' + value.substring(end);
        onChange(newValue);

        // Move cursor after inserted spaces
        requestAnimationFrame(() => {
          textarea.selectionStart = textarea.selectionEnd = start + 2;
        });
      }
    },
    [value, onChange]
  );

  return (
    <div className="space-y-2">
      {/* Editor */}
      <div
        ref={containerRef}
        className="relative border rounded-md overflow-hidden bg-slate-950 dark:bg-slate-900"
        style={{ height }}
      >
        {/* Line numbers */}
        <div
          ref={lineNumbersRef}
          className="absolute left-0 top-0 bottom-0 w-12 bg-slate-900 dark:bg-slate-800 border-r border-slate-700 overflow-hidden select-none"
          style={{ fontFamily: 'ui-monospace, monospace', fontSize: '13px', lineHeight: '1.5' }}
        >
          <div className="py-3 px-2 text-right">
            {lines.map((num) => (
              <div
                key={num}
                className={`h-[1.5em] ${
                  error && error.line === num
                    ? 'text-red-400 font-bold'
                    : 'text-slate-500'
                }`}
              >
                {num}
              </div>
            ))}
          </div>
        </div>

        {/* Highlight layer */}
        <div
          ref={highlightRef}
          className="absolute left-12 top-0 right-0 bottom-0 overflow-hidden pointer-events-none"
          style={{ fontFamily: 'ui-monospace, monospace', fontSize: '13px', lineHeight: '1.5' }}
        >
          <div
            className="p-3 whitespace-pre"
            dangerouslySetInnerHTML={{ __html: highlightedHtml }}
          />
        </div>

        {/* Textarea (invisible but captures input) */}
        <textarea
          ref={textareaRef}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          onScroll={handleScroll}
          onKeyDown={handleKeyDown}
          placeholder={placeholder}
          readOnly={readOnly}
          spellCheck={false}
          className="absolute left-12 top-0 right-0 bottom-0 p-3 bg-transparent text-transparent caret-white resize-none outline-none"
          style={{
            fontFamily: 'ui-monospace, monospace',
            fontSize: '13px',
            lineHeight: '1.5',
            caretColor: 'white',
          }}
        />
      </div>

      {/* Validation status */}
      <div className="flex items-center gap-2 text-sm">
        {value.trim() === '' ? (
          <span className="text-muted-foreground">Enter JSON content</span>
        ) : error ? (
          <div className="flex items-center gap-2 text-red-500">
            <AlertCircle className="h-4 w-4" />
            <span>
              Line {error.line}, Column {error.column}: {error.message}
            </span>
          </div>
        ) : (
          <div className="flex items-center gap-2 text-green-500">
            <Check className="h-4 w-4" />
            <span>Valid JSON</span>
          </div>
        )}
      </div>

      {/* CSS for syntax highlighting */}
      <style>{`
        .json-line {
          min-height: 1.5em;
          color: #e2e8f0; /* Default text color for punctuation */
        }
        .json-error-line {
          background-color: rgba(239, 68, 68, 0.2);
          border-left: 3px solid #ef4444;
          margin-left: -3px;
          padding-left: 3px;
        }
        .json-string {
          color: #a5d6ff;
        }
        .json-key {
          color: #7ee787;
        }
        .json-number {
          color: #ffa657;
        }
        .json-keyword {
          color: #ff7b72;
        }
      `}</style>
    </div>
  );
}

export default JsonEditor;
