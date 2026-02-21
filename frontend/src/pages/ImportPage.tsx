import { useState } from 'react';
import { useImportYaml } from '@/api/apps';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Upload, FileText, CheckCircle2, AlertTriangle } from 'lucide-react';

export default function ImportPage() {
  const [yaml, setYaml] = useState('');
  const [siteId, setSiteId] = useState('');
  const importMutation = useImportYaml();

  const handleFileUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = (ev) => {
      setYaml(ev.target?.result as string);
    };
    reader.readAsText(file);
  };

  const handleImport = () => {
    if (!yaml || !siteId) return;
    importMutation.mutate({ yaml, site_id: siteId });
  };

  return (
    <div className="container mx-auto p-6 max-w-4xl">
      <h1 className="text-2xl font-bold mb-6">Import Application Map</h1>
      <p className="text-muted-foreground mb-6">
        Import an old AppControl YAML map to create a new application with all its components,
        groups, variables, commands, dependencies, and links.
      </p>

      <div className="grid gap-6">
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Upload className="h-5 w-5" />
              Upload YAML Map
            </CardTitle>
            <CardDescription>
              Select a YAML file or paste the content directly
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div>
              <label className="block text-sm font-medium mb-1">Site ID</label>
              <input
                type="text"
                value={siteId}
                onChange={(e) => setSiteId(e.target.value)}
                placeholder="UUID of the target site"
                className="w-full px-3 py-2 border rounded-md bg-background text-sm"
              />
            </div>
            <div>
              <label className="block text-sm font-medium mb-1">YAML File</label>
              <input
                type="file"
                accept=".yaml,.yml"
                onChange={handleFileUpload}
                className="block w-full text-sm file:mr-4 file:py-2 file:px-4 file:rounded-md file:border-0 file:text-sm file:font-medium file:bg-primary file:text-primary-foreground hover:file:bg-primary/90"
              />
            </div>
            <div>
              <label className="block text-sm font-medium mb-1">
                Or paste YAML content
              </label>
              <textarea
                value={yaml}
                onChange={(e) => setYaml(e.target.value)}
                placeholder="application:&#10;  name: My Application&#10;  components:&#10;    - name: database&#10;      ..."
                className="w-full h-64 px-3 py-2 border rounded-md bg-background text-sm font-mono"
              />
            </div>
            <Button
              onClick={handleImport}
              disabled={!yaml || !siteId || importMutation.isPending}
              className="w-full"
            >
              {importMutation.isPending ? 'Importing...' : 'Import Map'}
            </Button>
          </CardContent>
        </Card>

        {importMutation.isSuccess && (
          <Card className="border-green-200 bg-green-50">
            <CardHeader>
              <CardTitle className="flex items-center gap-2 text-green-700">
                <CheckCircle2 className="h-5 w-5" />
                Import Successful
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="grid grid-cols-2 gap-2 text-sm">
                <div><FileText className="inline h-4 w-4 mr-1" />Application: <strong>{importMutation.data?.application_name}</strong></div>
                <div>Components: <strong>{importMutation.data?.components_created}</strong></div>
                <div>Groups: <strong>{importMutation.data?.groups_created}</strong></div>
                <div>Variables: <strong>{importMutation.data?.variables_created}</strong></div>
                <div>Commands: <strong>{importMutation.data?.commands_created}</strong></div>
                <div>Dependencies: <strong>{importMutation.data?.dependencies_created}</strong></div>
                <div>Links: <strong>{importMutation.data?.links_created}</strong></div>
              </div>
              {importMutation.data?.warnings?.length > 0 && (
                <div className="mt-4">
                  <h4 className="font-medium text-amber-700 flex items-center gap-1">
                    <AlertTriangle className="h-4 w-4" />
                    Warnings
                  </h4>
                  <ul className="text-sm text-amber-600 list-disc list-inside mt-1">
                    {importMutation.data.warnings.map((w: string, i: number) => (
                      <li key={i}>{w}</li>
                    ))}
                  </ul>
                </div>
              )}
            </CardContent>
          </Card>
        )}

        {importMutation.isError && (
          <Card className="border-red-200 bg-red-50">
            <CardHeader>
              <CardTitle className="flex items-center gap-2 text-red-700">
                <AlertTriangle className="h-5 w-5" />
                Import Failed
              </CardTitle>
            </CardHeader>
            <CardContent>
              <p className="text-sm text-red-600">
                {importMutation.error?.message || 'An error occurred during import'}
              </p>
            </CardContent>
          </Card>
        )}
      </div>
    </div>
  );
}
