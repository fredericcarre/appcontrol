import { useState } from 'react';
import { Plus, Trash2, Play, Save } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { useDiscoveryStore } from '@/stores/discovery';
import type { CustomAction } from './TopologyMap.types';

interface CustomActionEditorProps {
  serviceIndex: number;
}

function ActionRow({
  action,
  onRemove,
}: {
  action: CustomAction;
  onRemove: () => void;
}) {
  return (
    <div className="flex items-center gap-1.5 text-[11px] group">
      <span className="text-blue-600 font-medium flex-shrink-0">{action.name}:</span>
      <code className="text-muted-foreground font-mono truncate flex-1" title={action.command}>
        {action.command}
      </code>
      <Button
        size="icon"
        variant="ghost"
        className="h-5 w-5 opacity-0 group-hover:opacity-100 text-destructive hover:bg-destructive/10"
        onClick={onRemove}
        title="Remove action"
      >
        <Trash2 className="h-3 w-3" />
      </Button>
    </div>
  );
}

export function CustomActionEditor({ serviceIndex }: CustomActionEditorProps) {
  const serviceEdits = useDiscoveryStore((s) => s.serviceEdits);
  const updateServiceEdit = useDiscoveryStore((s) => s.updateServiceEdit);

  const [isAdding, setIsAdding] = useState(false);
  const [newName, setNewName] = useState('');
  const [newCommand, setNewCommand] = useState('');

  const currentEdits = serviceEdits.get(serviceIndex);
  const customActions = currentEdits?.customActions || [];

  const handleAdd = () => {
    if (!newName.trim() || !newCommand.trim()) return;

    const newAction: CustomAction = {
      name: newName.trim(),
      command: newCommand.trim(),
    };

    updateServiceEdit(serviceIndex, {
      customActions: [...customActions, newAction],
    });

    setNewName('');
    setNewCommand('');
    setIsAdding(false);
  };

  const handleRemove = (index: number) => {
    const updated = customActions.filter((_, i) => i !== index);
    updateServiceEdit(serviceIndex, { customActions: updated });
  };

  return (
    <div>
      <div className="text-[10px] font-medium text-muted-foreground uppercase tracking-wider mb-2 flex items-center justify-between">
        <span className="flex items-center gap-1">
          <Play className="h-3 w-3 text-blue-500" />
          CUSTOM ACTIONS ({customActions.length})
        </span>
        {!isAdding && (
          <Button
            size="sm"
            variant="ghost"
            className="h-5 text-[10px] gap-0.5 px-1"
            onClick={() => setIsAdding(true)}
          >
            <Plus className="h-3 w-3" />
            Add
          </Button>
        )}
      </div>

      <div className="space-y-1.5 pl-2 border-l-2 border-border">
        {/* Existing actions */}
        {customActions.map((action, i) => (
          <ActionRow key={i} action={action} onRemove={() => handleRemove(i)} />
        ))}

        {/* Add form */}
        {isAdding && (
          <div className="space-y-1.5 pt-1 pb-1 border-t border-dashed border-border mt-2">
            <Input
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              placeholder="Action name (e.g., health_check)"
              className="h-6 text-[11px] font-medium"
              autoFocus
            />
            <Input
              value={newCommand}
              onChange={(e) => setNewCommand(e.target.value)}
              placeholder="Command (e.g., curl -s http://localhost:8080/health)"
              className="h-6 text-[11px] font-mono"
              onKeyDown={(e) => {
                if (e.key === 'Enter') handleAdd();
                if (e.key === 'Escape') {
                  setIsAdding(false);
                  setNewName('');
                  setNewCommand('');
                }
              }}
            />
            <div className="flex justify-end gap-1">
              <Button
                size="sm"
                variant="ghost"
                className="h-5 text-[10px] px-1"
                onClick={() => {
                  setIsAdding(false);
                  setNewName('');
                  setNewCommand('');
                }}
              >
                Cancel
              </Button>
              <Button
                size="sm"
                className="h-5 text-[10px] px-1.5 gap-0.5"
                onClick={handleAdd}
                disabled={!newName.trim() || !newCommand.trim()}
              >
                <Save className="h-3 w-3" />
                Save
              </Button>
            </div>
          </div>
        )}

        {/* Empty state */}
        {customActions.length === 0 && !isAdding && (
          <div className="text-[11px] text-muted-foreground italic">
            No custom actions defined
          </div>
        )}
      </div>
    </div>
  );
}
