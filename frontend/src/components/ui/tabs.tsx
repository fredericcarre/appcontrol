import { createContext, useContext, useState, ReactNode, HTMLAttributes } from 'react';
import { cn } from '@/lib/utils';

interface TabsContextType {
  value: string;
  setValue: (v: string) => void;
}

const TabsContext = createContext<TabsContextType>({ value: '', setValue: () => {} });

export function Tabs({ defaultValue, value, onValueChange, children, className, ...props }: { defaultValue?: string; value?: string; onValueChange?: (v: string) => void; children: ReactNode } & HTMLAttributes<HTMLDivElement>) {
  const [internalValue, setInternalValue] = useState(defaultValue || '');
  const current = value ?? internalValue;
  const setCurrent = onValueChange ?? setInternalValue;

  return (
    <TabsContext.Provider value={{ value: current, setValue: setCurrent }}>
      <div className={className} {...props}>{children}</div>
    </TabsContext.Provider>
  );
}

export function TabsList({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div className={cn('inline-flex h-10 items-center justify-center rounded-md bg-muted p-1 text-muted-foreground', className)} {...props} />
  );
}

export function TabsTrigger({ value, className, ...props }: { value: string } & HTMLAttributes<HTMLButtonElement>) {
  const ctx = useContext(TabsContext);
  return (
    <button
      type="button"
      className={cn(
        'inline-flex items-center justify-center whitespace-nowrap rounded-sm px-3 py-1.5 text-sm font-medium transition-all',
        ctx.value === value ? 'bg-background text-foreground shadow-sm' : 'hover:bg-background/50',
        className,
      )}
      onClick={() => ctx.setValue(value)}
      {...props}
    />
  );
}

export function TabsContent({ value, className, ...props }: { value: string } & HTMLAttributes<HTMLDivElement>) {
  const ctx = useContext(TabsContext);
  if (ctx.value !== value) return null;
  return <div className={cn('mt-2', className)} {...props} />;
}
