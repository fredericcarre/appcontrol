import { useState } from 'react';
import {
  useHostings,
  useCreateHosting,
  useUpdateHosting,
  useDeleteHosting,
  useHostingSites,
  type Hosting,
} from '@/api/hostings';
import { useAuthStore } from '@/stores/auth';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  Warehouse,
  Plus,
  MoreHorizontal,
  Pencil,
  Trash2,
  MapPin,
  ChevronDown,
  ChevronRight,
  Building2,
  Server,
  FlaskConical,
  Code2,
} from 'lucide-react';

const SITE_TYPE_INFO: Record<string, { label: string; icon: typeof Building2; color: string }> = {
  primary: { label: 'Primary', icon: Building2, color: 'bg-blue-600' },
  dr: { label: 'DR', icon: Server, color: 'bg-orange-600' },
  staging: { label: 'Staging', icon: FlaskConical, color: 'bg-purple-600' },
  development: { label: 'Dev', icon: Code2, color: 'bg-green-600' },
};

interface HostingFormData {
  name: string;
  description: string;
}

const defaultFormData: HostingFormData = {
  name: '',
  description: '',
};

function HostingSitesRow({ hostingId }: { hostingId: string }) {
  const { data: sites, isLoading } = useHostingSites(hostingId);

  if (isLoading) {
    return (
      <TableRow>
        <TableCell colSpan={5} className="pl-12 text-muted-foreground text-sm">
          Loading sites...
        </TableCell>
      </TableRow>
    );
  }

  if (!sites || sites.length === 0) {
    return (
      <TableRow>
        <TableCell colSpan={5} className="pl-12 text-muted-foreground text-sm italic">
          No sites assigned to this hosting
        </TableCell>
      </TableRow>
    );
  }

  return (
    <>
      {sites.map((site) => {
        const typeInfo = SITE_TYPE_INFO[site.site_type] || SITE_TYPE_INFO.primary;
        const TypeIcon = typeInfo.icon;
        return (
          <TableRow key={site.id} className="bg-muted/30">
            <TableCell className="pl-12">
              <div className="flex items-center gap-2">
                <MapPin className="h-3.5 w-3.5 text-muted-foreground" />
                <span className="font-medium">{site.name}</span>
                <code className="bg-muted px-1.5 py-0.5 rounded text-xs">{site.code}</code>
              </div>
            </TableCell>
            <TableCell>
              <Badge className={`gap-1 text-xs ${typeInfo.color}`}>
                <TypeIcon className="h-3 w-3" />
                {typeInfo.label}
              </Badge>
            </TableCell>
            <TableCell className="text-muted-foreground text-sm">
              {site.location || '-'}
            </TableCell>
            <TableCell>
              {site.is_active ? (
                <Badge variant="default" className="bg-green-600 text-xs">Active</Badge>
              ) : (
                <Badge variant="secondary" className="text-xs">Inactive</Badge>
              )}
            </TableCell>
            <TableCell />
          </TableRow>
        );
      })}
    </>
  );
}

export function HostingsPage() {
  const user = useAuthStore((s) => s.user);
  const isAdmin = user?.role === 'admin';
  const { data: hostings, isLoading } = useHostings();
  const createHosting = useCreateHosting();
  const updateHosting = useUpdateHosting();
  const deleteHosting = useDeleteHosting();

  const [createOpen, setCreateOpen] = useState(false);
  const [editHosting, setEditHosting] = useState<Hosting | null>(null);
  const [deleteHostingConfirm, setDeleteHostingConfirm] = useState<Hosting | null>(null);
  const [formData, setFormData] = useState<HostingFormData>(defaultFormData);
  const [formError, setFormError] = useState<string | null>(null);
  const [expandedHostings, setExpandedHostings] = useState<Set<string>>(new Set());

  const toggleExpand = (id: string) => {
    setExpandedHostings((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  const openCreate = () => {
    setFormData(defaultFormData);
    setFormError(null);
    setCreateOpen(true);
  };

  const openEdit = (hosting: Hosting) => {
    setFormData({
      name: hosting.name,
      description: hosting.description || '',
    });
    setFormError(null);
    setEditHosting(hosting);
  };

  const handleCreate = async () => {
    if (!formData.name.trim()) {
      setFormError('Name is required');
      return;
    }
    try {
      await createHosting.mutateAsync({
        name: formData.name.trim(),
        description: formData.description.trim() || undefined,
      });
      setCreateOpen(false);
    } catch (err: unknown) {
      const error = err as { response?: { data?: { error?: string } } };
      setFormError(error.response?.data?.error || 'Failed to create hosting');
    }
  };

  const handleUpdate = async () => {
    if (!editHosting || !formData.name.trim()) {
      setFormError('Name is required');
      return;
    }
    try {
      await updateHosting.mutateAsync({
        id: editHosting.id,
        name: formData.name.trim(),
        description: formData.description.trim() || undefined,
      });
      setEditHosting(null);
    } catch (err: unknown) {
      const error = err as { response?: { data?: { error?: string } } };
      setFormError(error.response?.data?.error || 'Failed to update hosting');
    }
  };

  const handleDelete = async () => {
    if (!deleteHostingConfirm) return;
    try {
      await deleteHosting.mutateAsync(deleteHostingConfirm.id);
      setDeleteHostingConfirm(null);
    } catch (err: unknown) {
      const error = err as { response?: { data?: { error?: string } } };
      setFormError(error.response?.data?.error || 'Failed to delete hosting');
    }
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin h-8 w-8 border-2 border-primary border-t-transparent rounded-full" />
      </div>
    );
  }

  const hostingList = hostings || [];

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold">Hostings</h1>
          <p className="text-muted-foreground">
            Group sites by datacenter or hosting location
          </p>
        </div>
        {isAdmin && (
          <Button onClick={openCreate} className="gap-2">
            <Plus className="h-4 w-4" /> Add Hosting
          </Button>
        )}
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Warehouse className="h-5 w-5" />
            All Hostings ({hostingList.length})
          </CardTitle>
        </CardHeader>
        <CardContent>
          {hostingList.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-12 text-center">
              <Warehouse className="h-12 w-12 text-muted-foreground mb-4" />
              <h3 className="font-medium text-lg mb-2">No Hostings Configured</h3>
              <p className="text-muted-foreground max-w-md mb-4">
                Hostings represent datacenter locations or cloud regions. Create a hosting
                to group related sites together. This helps organize DR switchover targets
                and understand site proximity.
              </p>
              {isAdmin && (
                <Button onClick={openCreate} className="gap-2">
                  <Plus className="h-4 w-4" /> Create First Hosting
                </Button>
              )}
            </div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Type / Description</TableHead>
                  <TableHead>Location</TableHead>
                  <TableHead>Status</TableHead>
                  {isAdmin && <TableHead className="w-[50px]" />}
                </TableRow>
              </TableHeader>
              <TableBody>
                {hostingList.map((hosting) => {
                  const isExpanded = expandedHostings.has(hosting.id);
                  return (
                    <>
                      <TableRow
                        key={hosting.id}
                        className="cursor-pointer hover:bg-muted/50"
                        onClick={() => toggleExpand(hosting.id)}
                      >
                        <TableCell className="font-medium">
                          <div className="flex items-center gap-2">
                            {isExpanded ? (
                              <ChevronDown className="h-4 w-4 text-muted-foreground" />
                            ) : (
                              <ChevronRight className="h-4 w-4 text-muted-foreground" />
                            )}
                            <Warehouse className="h-4 w-4" />
                            {hosting.name}
                          </div>
                        </TableCell>
                        <TableCell className="text-muted-foreground">
                          {hosting.description || '-'}
                        </TableCell>
                        <TableCell />
                        <TableCell />
                        {isAdmin && (
                          <TableCell>
                            <DropdownMenu>
                              <DropdownMenuTrigger asChild>
                                <Button
                                  variant="ghost"
                                  size="icon"
                                  className="h-8 w-8"
                                  onClick={(e) => e.stopPropagation()}
                                >
                                  <MoreHorizontal className="h-4 w-4" />
                                </Button>
                              </DropdownMenuTrigger>
                              <DropdownMenuContent align="end">
                                <DropdownMenuItem onClick={() => openEdit(hosting)}>
                                  <Pencil className="h-4 w-4 mr-2" />
                                  Edit
                                </DropdownMenuItem>
                                <DropdownMenuItem
                                  onClick={() => setDeleteHostingConfirm(hosting)}
                                  className="text-destructive focus:text-destructive"
                                >
                                  <Trash2 className="h-4 w-4 mr-2" />
                                  Delete
                                </DropdownMenuItem>
                              </DropdownMenuContent>
                            </DropdownMenu>
                          </TableCell>
                        )}
                      </TableRow>
                      {isExpanded && (
                        <HostingSitesRow hostingId={hosting.id} />
                      )}
                    </>
                  );
                })}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-lg">About Hostings</CardTitle>
        </CardHeader>
        <CardContent className="text-sm text-muted-foreground space-y-2">
          <p>
            <strong>Hostings</strong> represent physical datacenter locations or cloud regions
            that contain one or more sites. They help organize your infrastructure geographically.
          </p>
          <p>
            <strong>Switchover:</strong> Site failover typically happens within the same hosting
            (e.g., primary to DR site in the same datacenter). Cross-hosting switchover is also
            supported for failover between datacenters.
          </p>
          <p>
            Assign sites to hostings from the <strong>Sites</strong> page.
          </p>
        </CardContent>
      </Card>

      {/* Create Hosting Dialog */}
      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Plus className="h-5 w-5" />
              Create Hosting
            </DialogTitle>
            <DialogDescription>
              Add a new hosting location to group related sites.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4 py-4">
            {formError && (
              <div className="text-sm text-destructive bg-destructive/10 p-2 rounded">
                {formError}
              </div>
            )}
            <div className="space-y-2">
              <Label htmlFor="name">Name</Label>
              <Input
                id="name"
                placeholder="e.g., Datacenter Paris, AWS eu-west-1"
                value={formData.name}
                onChange={(e) => setFormData({ ...formData, name: e.target.value })}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="description">Description (optional)</Label>
              <Textarea
                id="description"
                placeholder="e.g., Primary datacenter in Ile-de-France region"
                value={formData.description}
                onChange={(e) => setFormData({ ...formData, description: e.target.value })}
                rows={3}
              />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setCreateOpen(false)}>
              Cancel
            </Button>
            <Button onClick={handleCreate} disabled={createHosting.isPending}>
              {createHosting.isPending ? 'Creating...' : 'Create Hosting'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Edit Hosting Dialog */}
      <Dialog open={!!editHosting} onOpenChange={(open) => !open && setEditHosting(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Pencil className="h-5 w-5" />
              Edit Hosting
            </DialogTitle>
            <DialogDescription>
              Update hosting information.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4 py-4">
            {formError && (
              <div className="text-sm text-destructive bg-destructive/10 p-2 rounded">
                {formError}
              </div>
            )}
            <div className="space-y-2">
              <Label htmlFor="edit-name">Name</Label>
              <Input
                id="edit-name"
                value={formData.name}
                onChange={(e) => setFormData({ ...formData, name: e.target.value })}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="edit-description">Description</Label>
              <Textarea
                id="edit-description"
                value={formData.description}
                onChange={(e) => setFormData({ ...formData, description: e.target.value })}
                rows={3}
              />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setEditHosting(null)}>
              Cancel
            </Button>
            <Button onClick={handleUpdate} disabled={updateHosting.isPending}>
              {updateHosting.isPending ? 'Saving...' : 'Save Changes'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Delete Confirmation Dialog */}
      <Dialog open={!!deleteHostingConfirm} onOpenChange={(open) => !open && setDeleteHostingConfirm(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Trash2 className="h-5 w-5 text-destructive" />
              Delete Hosting
            </DialogTitle>
            <DialogDescription>
              Are you sure you want to delete the hosting{' '}
              <span className="font-medium">{deleteHostingConfirm?.name}</span>?
              This action cannot be undone.
            </DialogDescription>
          </DialogHeader>
          {formError && (
            <div className="text-sm text-destructive bg-destructive/10 p-2 rounded">
              {formError}
            </div>
          )}
          <p className="text-sm text-muted-foreground">
            Note: You cannot delete a hosting that has sites assigned to it. Unassign sites first.
          </p>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDeleteHostingConfirm(null)}>
              Cancel
            </Button>
            <Button variant="destructive" onClick={handleDelete} disabled={deleteHosting.isPending}>
              {deleteHosting.isPending ? 'Deleting...' : 'Delete Hosting'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
