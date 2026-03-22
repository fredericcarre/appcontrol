import { useState } from 'react';
import {
  useSites,
  useCreateSite,
  useUpdateSite,
  useDeleteSite,
  type Site,
} from '@/api/sites';
import { useAuthStore } from '@/stores/auth';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
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
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  MapPin,
  Plus,
  MoreHorizontal,
  Pencil,
  Trash2,
  ToggleLeft,
  ToggleRight,
  Building2,
  Server,
  FlaskConical,
  Code2,
} from 'lucide-react';

const SITE_TYPES = [
  { value: 'primary', label: 'Primary', icon: Building2, color: 'bg-blue-600' },
  { value: 'dr', label: 'DR (Disaster Recovery)', icon: Server, color: 'bg-orange-600' },
  { value: 'staging', label: 'Staging', icon: FlaskConical, color: 'bg-purple-600' },
  { value: 'development', label: 'Development', icon: Code2, color: 'bg-green-600' },
] as const;

function getSiteTypeInfo(type: string) {
  return SITE_TYPES.find((t) => t.value === type) || SITE_TYPES[0];
}

interface SiteFormData {
  name: string;
  code: string;
  site_type: string;
  location: string;
}

const defaultFormData: SiteFormData = {
  name: '',
  code: '',
  site_type: 'primary',
  location: '',
};

export function SitesPage() {
  const user = useAuthStore((s) => s.user);
  const isAdmin = user?.role === 'admin';
  const { data: sites, isLoading } = useSites();
  const createSite = useCreateSite();
  const updateSite = useUpdateSite();
  const deleteSite = useDeleteSite();

  const [createOpen, setCreateOpen] = useState(false);
  const [editSite, setEditSite] = useState<Site | null>(null);
  const [deleteSiteConfirm, setDeleteSiteConfirm] = useState<Site | null>(null);
  const [formData, setFormData] = useState<SiteFormData>(defaultFormData);
  const [formError, setFormError] = useState<string | null>(null);

  const isMutating = createSite.isPending || updateSite.isPending || deleteSite.isPending;

  const openCreate = () => {
    setFormData(defaultFormData);
    setFormError(null);
    setCreateOpen(true);
  };

  const openEdit = (site: Site) => {
    setFormData({
      name: site.name,
      code: site.code,
      site_type: site.site_type,
      location: site.location || '',
    });
    setFormError(null);
    setEditSite(site);
  };

  const handleCreate = async () => {
    if (!formData.name.trim() || !formData.code.trim()) {
      setFormError('Name and code are required');
      return;
    }
    try {
      await createSite.mutateAsync({
        name: formData.name.trim(),
        code: formData.code.trim().toLowerCase(),
        site_type: formData.site_type,
        location: formData.location.trim() || undefined,
      });
      setCreateOpen(false);
    } catch (err: unknown) {
      const error = err as { response?: { data?: { error?: string } } };
      setFormError(error.response?.data?.error || 'Failed to create site');
    }
  };

  const handleUpdate = async () => {
    if (!editSite || !formData.name.trim()) {
      setFormError('Name is required');
      return;
    }
    try {
      await updateSite.mutateAsync({
        id: editSite.id,
        name: formData.name.trim(),
        location: formData.location.trim() || undefined,
      });
      setEditSite(null);
    } catch (err: unknown) {
      const error = err as { response?: { data?: { error?: string } } };
      setFormError(error.response?.data?.error || 'Failed to update site');
    }
  };

  const handleToggleActive = async (site: Site) => {
    await updateSite.mutateAsync({
      id: site.id,
      is_active: !site.is_active,
    });
  };

  const handleDelete = async () => {
    if (!deleteSiteConfirm) return;
    try {
      await deleteSite.mutateAsync(deleteSiteConfirm.id);
      setDeleteSiteConfirm(null);
    } catch (err: unknown) {
      const error = err as { response?: { data?: { error?: string } } };
      setFormError(error.response?.data?.error || 'Failed to delete site');
    }
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin h-8 w-8 border-2 border-primary border-t-transparent rounded-full" />
      </div>
    );
  }

  const siteList = sites || [];

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold">Sites</h1>
          <p className="text-muted-foreground">
            Manage datacenter locations for gateways and applications
          </p>
        </div>
        {isAdmin && (
          <Button onClick={openCreate} className="gap-2">
            <Plus className="h-4 w-4" /> Add Site
          </Button>
        )}
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <MapPin className="h-5 w-5" />
            All Sites ({siteList.length})
          </CardTitle>
        </CardHeader>
        <CardContent>
          {siteList.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-12 text-center">
              <MapPin className="h-12 w-12 text-muted-foreground mb-4" />
              <h3 className="font-medium text-lg mb-2">No Sites Configured</h3>
              <p className="text-muted-foreground max-w-md mb-4">
                Sites represent physical or logical locations (datacenters) where your infrastructure runs.
                Create your first site to organize gateways and enable DR capabilities.
              </p>
              {isAdmin && (
                <Button onClick={openCreate} className="gap-2">
                  <Plus className="h-4 w-4" /> Create First Site
                </Button>
              )}
            </div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Code</TableHead>
                  <TableHead>Type</TableHead>
                  <TableHead>Location</TableHead>
                  <TableHead>Status</TableHead>
                  {isAdmin && <TableHead className="w-[50px]"></TableHead>}
                </TableRow>
              </TableHeader>
              <TableBody>
                {siteList.map((site) => {
                  const typeInfo = getSiteTypeInfo(site.site_type);
                  const TypeIcon = typeInfo.icon;
                  return (
                    <TableRow key={site.id}>
                      <TableCell className="font-medium">{site.name}</TableCell>
                      <TableCell>
                        <code className="bg-muted px-1.5 py-0.5 rounded text-sm">
                          {site.code}
                        </code>
                      </TableCell>
                      <TableCell>
                        <Badge className={`gap-1 ${typeInfo.color}`}>
                          <TypeIcon className="h-3 w-3" />
                          {typeInfo.label}
                        </Badge>
                      </TableCell>
                      <TableCell className="text-muted-foreground">
                        {site.location || '-'}
                      </TableCell>
                      <TableCell>
                        {site.is_active ? (
                          <Badge variant="default" className="bg-green-600">Active</Badge>
                        ) : (
                          <Badge variant="secondary">Inactive</Badge>
                        )}
                      </TableCell>
                      {isAdmin && (
                        <TableCell>
                          <DropdownMenu>
                            <DropdownMenuTrigger asChild>
                              <Button variant="ghost" size="icon" className="h-8 w-8">
                                <MoreHorizontal className="h-4 w-4" />
                              </Button>
                            </DropdownMenuTrigger>
                            <DropdownMenuContent align="end">
                              <DropdownMenuItem onClick={() => openEdit(site)}>
                                <Pencil className="h-4 w-4 mr-2" />
                                Edit
                              </DropdownMenuItem>
                              <DropdownMenuItem
                                onClick={() => handleToggleActive(site)}
                                disabled={isMutating}
                              >
                                {site.is_active ? (
                                  <>
                                    <ToggleLeft className="h-4 w-4 mr-2" />
                                    Deactivate
                                  </>
                                ) : (
                                  <>
                                    <ToggleRight className="h-4 w-4 mr-2" />
                                    Activate
                                  </>
                                )}
                              </DropdownMenuItem>
                              <DropdownMenuItem
                                onClick={() => setDeleteSiteConfirm(site)}
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
                  );
                })}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-lg">About Sites</CardTitle>
        </CardHeader>
        <CardContent className="text-sm text-muted-foreground space-y-2">
          <p>
            <strong>Sites</strong> represent physical or logical locations (datacenters) where infrastructure runs.
            Each gateway and application is assigned to a site.
          </p>
          <p>
            <strong>Site Types:</strong>
          </p>
          <ul className="list-disc list-inside space-y-1 ml-2">
            <li><strong>Primary</strong> - Main production site</li>
            <li><strong>DR</strong> - Disaster Recovery site for failover</li>
            <li><strong>Staging</strong> - Pre-production testing environment</li>
            <li><strong>Development</strong> - Development and testing</li>
          </ul>
        </CardContent>
      </Card>

      {/* Create Site Dialog */}
      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Plus className="h-5 w-5" />
              Create Site
            </DialogTitle>
            <DialogDescription>
              Add a new site to organize your infrastructure.
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
                placeholder="e.g., Production Paris"
                value={formData.name}
                onChange={(e) => setFormData({ ...formData, name: e.target.value })}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="code">Code</Label>
              <Input
                id="code"
                placeholder="e.g., prod-paris"
                value={formData.code}
                onChange={(e) => setFormData({ ...formData, code: e.target.value })}
              />
              <p className="text-xs text-muted-foreground">
                Short unique identifier (lowercase, no spaces)
              </p>
            </div>
            <div className="space-y-2">
              <Label htmlFor="site_type">Type</Label>
              <Select
                value={formData.site_type}
                onValueChange={(v) => setFormData({ ...formData, site_type: v })}
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {SITE_TYPES.map((type) => (
                    <SelectItem key={type.value} value={type.value}>
                      <span className="flex items-center gap-2">
                        <type.icon className="h-4 w-4" />
                        {type.label}
                      </span>
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-2">
              <Label htmlFor="location">Location (optional)</Label>
              <Input
                id="location"
                placeholder="e.g., AWS eu-west-3, On-premise DC1"
                value={formData.location}
                onChange={(e) => setFormData({ ...formData, location: e.target.value })}
              />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setCreateOpen(false)}>
              Cancel
            </Button>
            <Button onClick={handleCreate} disabled={createSite.isPending}>
              {createSite.isPending ? 'Creating...' : 'Create Site'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Edit Site Dialog */}
      <Dialog open={!!editSite} onOpenChange={(open) => !open && setEditSite(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Pencil className="h-5 w-5" />
              Edit Site
            </DialogTitle>
            <DialogDescription>
              Update site information. Code and type cannot be changed.
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
              <Label>Code</Label>
              <Input value={editSite?.code || ''} disabled />
              <p className="text-xs text-muted-foreground">Code cannot be changed</p>
            </div>
            <div className="space-y-2">
              <Label>Type</Label>
              <Input value={getSiteTypeInfo(editSite?.site_type || '').label} disabled />
              <p className="text-xs text-muted-foreground">Type cannot be changed</p>
            </div>
            <div className="space-y-2">
              <Label htmlFor="edit-location">Location</Label>
              <Input
                id="edit-location"
                placeholder="e.g., AWS eu-west-3"
                value={formData.location}
                onChange={(e) => setFormData({ ...formData, location: e.target.value })}
              />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setEditSite(null)}>
              Cancel
            </Button>
            <Button onClick={handleUpdate} disabled={updateSite.isPending}>
              {updateSite.isPending ? 'Saving...' : 'Save Changes'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Delete Confirmation Dialog */}
      <Dialog open={!!deleteSiteConfirm} onOpenChange={(open) => !open && setDeleteSiteConfirm(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Trash2 className="h-5 w-5 text-destructive" />
              Delete Site
            </DialogTitle>
            <DialogDescription>
              Are you sure you want to delete the site{' '}
              <span className="font-medium">{deleteSiteConfirm?.name}</span>?
              This action cannot be undone.
            </DialogDescription>
          </DialogHeader>
          {formError && (
            <div className="text-sm text-destructive bg-destructive/10 p-2 rounded">
              {formError}
            </div>
          )}
          <p className="text-sm text-muted-foreground">
            Note: You cannot delete a site that has applications or gateways assigned to it.
          </p>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDeleteSiteConfirm(null)}>
              Cancel
            </Button>
            <Button variant="destructive" onClick={handleDelete} disabled={deleteSite.isPending}>
              {deleteSite.isPending ? 'Deleting...' : 'Delete Site'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
