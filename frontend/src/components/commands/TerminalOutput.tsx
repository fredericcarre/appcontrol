import { forwardRef, HTMLAttributes } from 'react';
import { cn } from '@/lib/utils';

interface TerminalOutputProps extends HTMLAttributes<HTMLDivElement> {
  lines: Array<{ text: string; type?: 'stdout' | 'stderr' | 'info' }>;
}

const TerminalOutput = forwardRef<HTMLDivElement, TerminalOutputProps>(
  ({ lines, className, ...props }, ref) => (
    <div
      ref={ref}
      className={cn('bg-gray-950 text-gray-100 rounded-md p-4 font-mono text-xs overflow-auto', className)}
      {...props}
    >
      {lines.map((line, i) => (
        <div key={i} className="whitespace-pre-wrap">
          <span className={
            line.type === 'stderr' ? 'text-red-400' :
            line.type === 'info' ? 'text-blue-400' :
            'text-green-300'
          }>
            {line.text}
          </span>
        </div>
      ))}
    </div>
  ),
);
TerminalOutput.displayName = 'TerminalOutput';

export { TerminalOutput };
