import { useState, useRef, useEffect, ReactNode, HTMLAttributes } from 'react';
import { cn } from '@/lib/utils';
import { ChevronDown } from 'lucide-react';

export function Select({ value, onValueChange, children }: { value: string; onValueChange: (v: string) => void; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    function handleClick(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    document.addEventListener('mousedown', handleClick);
    return () => document.removeEventListener('mousedown', handleClick);
  }, []);

  return (
    <SelectContext.Provider value={{ value, onValueChange, open, setOpen }}>
      <div ref={ref} className="relative">
        {children}
      </div>
    </SelectContext.Provider>
  );
}

import { createContext, useContext } from 'react';

interface SelectContextType {
  value: string;
  onValueChange: (v: string) => void;
  open: boolean;
  setOpen: (o: boolean) => void;
}

const SelectContext = createContext<SelectContextType>({ value: '', onValueChange: () => {}, open: false, setOpen: () => {} });

export function SelectTrigger({ className, children, ...props }: { children: ReactNode } & HTMLAttributes<HTMLButtonElement>) {
  const ctx = useContext(SelectContext);
  return (
    <button
      type="button"
      className={cn('flex h-10 w-full items-center justify-between rounded-md border border-input bg-background px-3 py-2 text-sm', className)}
      onClick={() => ctx.setOpen(!ctx.open)}
      {...props}
    >
      {children}
      <ChevronDown className="h-4 w-4 opacity-50" />
    </button>
  );
}

export function SelectValue({ placeholder, children }: { placeholder?: string; children?: ReactNode }) {
  const ctx = useContext(SelectContext);
  // If children are provided and there's a value, render children; otherwise show value or placeholder
  if (children && ctx.value) {
    return <span className="truncate">{children}</span>;
  }
  return <span className="truncate">{ctx.value || placeholder}</span>;
}

export function SelectContent({ children, className }: { children: ReactNode; className?: string }) {
  const ctx = useContext(SelectContext);
  if (!ctx.open) return null;
  return (
    <div className={cn('absolute z-50 mt-1 w-full min-w-[8rem] rounded-md border bg-background shadow-md max-h-[300px] overflow-y-auto', className)}>
      <div className="p-1">{children}</div>
    </div>
  );
}

export function SelectGroup({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div className={cn('py-1', className)}>
      {children}
    </div>
  );
}

export function SelectLabel({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div className={cn('px-2 py-1.5 text-xs font-semibold text-muted-foreground', className)}>
      {children}
    </div>
  );
}

export function SelectItem({ value, children, className }: { value: string; children: ReactNode; className?: string }) {
  const ctx = useContext(SelectContext);
  return (
    <button
      type="button"
      className={cn(
        'relative flex w-full cursor-default select-none items-center rounded-sm py-1.5 px-2 text-sm outline-none hover:bg-accent',
        ctx.value === value && 'bg-accent',
        className,
      )}
      onClick={() => { ctx.onValueChange(value); ctx.setOpen(false); }}
    >
      {children}
    </button>
  );
}
