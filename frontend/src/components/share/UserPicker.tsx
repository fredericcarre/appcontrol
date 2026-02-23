import { useState, useRef, useEffect } from 'react';
import { useSearchUsers, UserSearchResult } from '@/api/users';
import { Input } from '@/components/ui/input';
import { Avatar, AvatarFallback } from '@/components/ui/avatar';

interface UserPickerProps {
  onSelect: (user: UserSearchResult) => void;
  placeholder?: string;
}

export function UserPicker({ onSelect, placeholder = 'Search users by name or email...' }: UserPickerProps) {
  const [query, setQuery] = useState('');
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const { data: users, isLoading } = useSearchUsers(query, open);

  // Close dropdown when clicking outside
  useEffect(() => {
    const handleClick = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener('mousedown', handleClick);
    return () => document.removeEventListener('mousedown', handleClick);
  }, []);

  const handleSelect = (user: UserSearchResult) => {
    onSelect(user);
    setQuery('');
    setOpen(false);
  };

  return (
    <div className="relative flex-1" ref={ref}>
      <Input
        placeholder={placeholder}
        value={query}
        onChange={(e) => {
          setQuery(e.target.value);
          setOpen(true);
        }}
        onFocus={() => setOpen(true)}
      />
      {open && query.length >= 1 && (
        <div className="absolute z-50 top-full mt-1 w-full bg-popover border rounded-md shadow-md max-h-48 overflow-auto">
          {isLoading ? (
            <p className="p-3 text-sm text-muted-foreground">Searching...</p>
          ) : !users?.length ? (
            <p className="p-3 text-sm text-muted-foreground">No users found</p>
          ) : (
            users.map((u) => (
              <button
                key={u.id}
                className="flex items-center gap-2 w-full p-2 hover:bg-accent text-left text-sm"
                onClick={() => handleSelect(u)}
              >
                <Avatar className="h-6 w-6">
                  <AvatarFallback className="text-[10px]">
                    {(u.display_name || u.email).slice(0, 2).toUpperCase()}
                  </AvatarFallback>
                </Avatar>
                <div className="min-w-0 flex-1">
                  <p className="truncate font-medium">{u.display_name || u.email}</p>
                  {u.display_name && <p className="truncate text-xs text-muted-foreground">{u.email}</p>}
                </div>
              </button>
            ))
          )}
        </div>
      )}
    </div>
  );
}
