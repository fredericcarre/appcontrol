import { useState, useMemo } from 'react';
import { Plus, Pencil, Trash2, FolderOpen, X, Loader2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import { ConfirmDialog, useConfirmDialog } from '@/components/ui/confirm-dialog';

const PRESET_COLORS = [
  { value: '#EF4444', label: 'Red' },
  { value: '#F97316', label: 'Orange' },
  { value: '#EAB308', label: 'Yellow' },
  { value: '#22C55E', label: 'Green' },
  { value: '#06B6D4', label: 'Cyan' },
  { value: '#3B82F6', label: 'Blue' },
  { value: '#8B5CF6', label: 'Violet' },
  { value: '#EC4899', label: 'Pink' },
  { value: '#6366F1', label: 'Indigo' },
  { value: '#78716C', label: 'Stone' },
] as const;

const DEFAULT_COLOR = '#6366F1';

interface GroupPanelProps {
  groups: Array<{
    id: string;
    name: string;
    color: string | null;
    description: string | null;
    display_order: number;
  }>;
  components: Array<{ id: string; group_id?: string | null }>;
  onCreateGroup: (name: string, color: string, description?: string) => Promise<void>;
  onUpdateGroup: (groupId: string, name: string, color: string) => Promise<void>;
  onDeleteGroup: (groupId: string) => Promise<void>;
  disabled?: boolean;
}

function ColorPicker({
  value,
  onChange,
  disabled,
}: {
  value: string;
  onChange: (color: string) => void;
  disabled?: boolean;
}) {
  return (
    <div className="flex flex-wrap gap-1.5">
      {PRESET_COLORS.map((preset) => (
        <button
          key={preset.value}
          type="button"
          disabled={disabled}
          title={preset.label}
          aria-label={`Select ${preset.label}`}
          className={
            'h-6 w-6 rounded-full border-2 transition-transform hover:scale-110 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50' +
            (value === preset.value ? ' border-foreground scale-110' : ' border-transparent')
          }
          style={{ backgroundColor: preset.value }}
          onClick={() => onChange(preset.value)}
        />
      ))}
    </div>
  );
}

interface GroupFormState {
  name: string;
  color: string;
  description: string;
}

function GroupForm({
  initial,
  submitLabel,
  onSubmit,
  onCancel,
  loading,
  disabled,
}: {
  initial: GroupFormState;
  submitLabel: string;
  onSubmit: (state: GroupFormState) => void;
  onCancel: () => void;
  loading: boolean;
  disabled?: boolean;
}) {
  const [name, setName] = useState(initial.name);
  const [color, setColor] = useState(initial.color);
  const [description, setDescription] = useState(initial.description);

  const canSubmit = name.trim().length > 0 && !loading && !disabled;

  return (
    <div className="space-y-3 rounded-md border border-border bg-muted/30 p-3">
      <div className="space-y-1.5">
        <Label htmlFor="group-name" className="text-xs">
          Name
        </Label>
        <Input
          id="group-name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Group name"
          className="h-8 text-sm"
          disabled={loading || disabled}
          autoFocus
        />
      </div>
      <div className="space-y-1.5">
        <Label className="text-xs">Color</Label>
        <ColorPicker value={color} onChange={setColor} disabled={loading || disabled} />
      </div>
      <div className="space-y-1.5">
        <Label htmlFor="group-desc" className="text-xs">
          Description
          <span className="text-muted-foreground font-normal ml-1">(optional)</span>
        </Label>
        <Input
          id="group-desc"
          value={description}
          onChange={(e) => setDescription(e.target.value)}
          placeholder="Short description"
          className="h-8 text-sm"
          disabled={loading || disabled}
        />
      </div>
      <div className="flex gap-2 justify-end">
        <Button variant="ghost" size="sm" onClick={onCancel} disabled={loading}>
          Cancel
        </Button>
        <Button
          size="sm"
          onClick={() => onSubmit({ name: name.trim(), color, description: description.trim() })}
          disabled={!canSubmit}
        >
          {loading && <Loader2 className="h-3.5 w-3.5 mr-1 animate-spin" />}
          {submitLabel}
        </Button>
      </div>
    </div>
  );
}

export function GroupPanel({
  groups,
  components,
  onCreateGroup,
  onUpdateGroup,
  onDeleteGroup,
  disabled,
}: GroupPanelProps) {
  const [showCreateForm, setShowCreateForm] = useState(false);
  const [editingGroupId, setEditingGroupId] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const confirmDialog = useConfirmDialog();

  const componentCountByGroup = useMemo(() => {
    const counts = new Map<string, number>();
    for (const comp of components) {
      if (comp.group_id) {
        counts.set(comp.group_id, (counts.get(comp.group_id) ?? 0) + 1);
      }
    }
    return counts;
  }, [components]);

  const sortedGroups = useMemo(
    () => [...groups].sort((a, b) => a.display_order - b.display_order),
    [groups],
  );

  const handleCreate = async (form: GroupFormState) => {
    setError(null);
    setLoading(true);
    try {
      await onCreateGroup(form.name, form.color, form.description || undefined);
      setShowCreateForm(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create group');
    } finally {
      setLoading(false);
    }
  };

  const handleUpdate = async (groupId: string, form: GroupFormState) => {
    setError(null);
    setLoading(true);
    try {
      await onUpdateGroup(groupId, form.name, form.color);
      setEditingGroupId(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to update group');
    } finally {
      setLoading(false);
    }
  };

  const handleDeleteRequest = (group: { id: string; name: string }) => {
    const count = componentCountByGroup.get(group.id) ?? 0;
    confirmDialog.confirm({
      title: `Delete "${group.name}"?`,
      description:
        count > 0
          ? `This group contains ${count} component${count > 1 ? 's' : ''}. They will be ungrouped.`
          : 'This group has no components and will be permanently deleted.',
      confirmLabel: 'Delete',
      variant: 'destructive',
      onConfirm: async () => {
        setError(null);
        setLoading(true);
        try {
          await onDeleteGroup(group.id);
        } catch (err) {
          setError(err instanceof Error ? err.message : 'Failed to delete group');
        } finally {
          setLoading(false);
        }
      },
    });
  };

  return (
    <>
      <div className="flex flex-col h-full max-h-[480px] w-72">
        {/* Header */}
        <div className="flex items-center justify-between px-3 py-2 border-b border-border">
          <div className="flex items-center gap-1.5">
            <FolderOpen className="h-4 w-4 text-muted-foreground" />
            <span className="text-sm font-semibold">Groups</span>
            <Badge variant="outline" className="ml-1 text-[10px] h-4 px-1.5">
              {groups.length}
            </Badge>
          </div>
          {!showCreateForm && (
            <Button
              variant="ghost"
              size="icon"
              className="h-7 w-7"
              onClick={() => {
                setShowCreateForm(true);
                setEditingGroupId(null);
                setError(null);
              }}
              disabled={disabled || loading}
              title="Create group"
              aria-label="Create group"
            >
              <Plus className="h-4 w-4" />
            </Button>
          )}
        </div>

        {/* Error */}
        {error && (
          <div className="mx-3 mt-2 flex items-center gap-2 rounded-md border border-destructive/50 bg-destructive/10 px-2 py-1.5 text-xs text-destructive">
            <span className="flex-1">{error}</span>
            <button onClick={() => setError(null)} aria-label="Dismiss error">
              <X className="h-3 w-3" />
            </button>
          </div>
        )}

        {/* Create form */}
        {showCreateForm && (
          <div className="px-3 pt-3">
            <GroupForm
              initial={{ name: '', color: DEFAULT_COLOR, description: '' }}
              submitLabel="Create"
              onSubmit={handleCreate}
              onCancel={() => {
                setShowCreateForm(false);
                setError(null);
              }}
              loading={loading}
              disabled={disabled}
            />
          </div>
        )}

        {/* Group list */}
        <ScrollArea className="flex-1 min-h-0">
          <div className="p-3 space-y-1">
            {sortedGroups.length === 0 && !showCreateForm && (
              <p className="text-xs text-muted-foreground text-center py-6">
                No groups yet. Click <Plus className="inline h-3 w-3 -mt-0.5" /> to create one.
              </p>
            )}

            {sortedGroups.map((group) => {
              const count = componentCountByGroup.get(group.id) ?? 0;
              const isEditing = editingGroupId === group.id;

              if (isEditing) {
                return (
                  <GroupForm
                    key={group.id}
                    initial={{
                      name: group.name,
                      color: group.color ?? DEFAULT_COLOR,
                      description: group.description ?? '',
                    }}
                    submitLabel="Save"
                    onSubmit={(form) => handleUpdate(group.id, form)}
                    onCancel={() => {
                      setEditingGroupId(null);
                      setError(null);
                    }}
                    loading={loading}
                    disabled={disabled}
                  />
                );
              }

              return (
                <div
                  key={group.id}
                  className="flex items-center gap-2 rounded-md px-2 py-1.5 hover:bg-accent/50 group/row"
                >
                  {/* Color swatch */}
                  <div
                    className="h-3.5 w-3.5 rounded-sm shrink-0 border border-black/10"
                    style={{ backgroundColor: group.color ?? DEFAULT_COLOR }}
                    title={group.color ?? DEFAULT_COLOR}
                  />

                  {/* Name + count */}
                  <div className="flex-1 min-w-0">
                    <span className="text-sm truncate block">{group.name}</span>
                  </div>
                  <Badge variant="outline" className="text-[10px] h-4 px-1.5 shrink-0">
                    {count}
                  </Badge>

                  {/* Actions (visible on hover) */}
                  <div className="flex gap-0.5 opacity-0 group-hover/row:opacity-100 transition-opacity">
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-6 w-6"
                      onClick={() => {
                        setEditingGroupId(group.id);
                        setShowCreateForm(false);
                        setError(null);
                      }}
                      disabled={disabled || loading}
                      title="Edit group"
                      aria-label={`Edit ${group.name}`}
                    >
                      <Pencil className="h-3 w-3" />
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-6 w-6 text-destructive hover:text-destructive"
                      onClick={() => handleDeleteRequest(group)}
                      disabled={disabled || loading}
                      title="Delete group"
                      aria-label={`Delete ${group.name}`}
                    >
                      <Trash2 className="h-3 w-3" />
                    </Button>
                  </div>
                </div>
              );
            })}
          </div>
        </ScrollArea>
      </div>

      {/* Confirm delete dialog */}
      <ConfirmDialog
        open={confirmDialog.state.open}
        onOpenChange={confirmDialog.setOpen}
        title={confirmDialog.state.title}
        description={confirmDialog.state.description}
        confirmLabel={confirmDialog.state.confirmLabel}
        cancelLabel={confirmDialog.state.cancelLabel}
        variant={confirmDialog.state.variant}
        onConfirm={confirmDialog.state.onConfirm}
      />
    </>
  );
}
