import { useState } from 'react';
import { useImportYaml, useImportJson } from '@/api/apps';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Upload, FileText, CheckCircle2, AlertTriangle, FileJson, FileCode } from 'lucide-react';

export default function ImportPage() {
  const [yaml, setYaml] = useState('');
  const [json, setJson] = useState('');
  const [siteId, setSiteId] = useState('');
  const [activeTab, setActiveTab] = useState<'json' | 'yaml'>('json');

  const importYamlMutation = useImportYaml();
  const importJsonMutation = useImportJson();

  const activeMutation = activeTab === 'json' ? importJsonMutation : importYamlMutation;

  const handleFileUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = (ev) => {
      const content = ev.target?.result as string;
      if (activeTab === 'json') {
        setJson(content);
      } else {
        setYaml(content);
      }
    };
    reader.readAsText(file);
  };

  const handleImport = () => {
    if (!siteId) return;
    if (activeTab === 'json' && json) {
      importJsonMutation.mutate({ json, site_id: siteId });
    } else if (activeTab === 'yaml' && yaml) {
      importYamlMutation.mutate({ yaml, site_id: siteId });
    }
  };

  const currentContent = activeTab === 'json' ? json : yaml;

  return (
    <div className="container mx-auto p-6 max-w-4xl">
      <h1 className="text-2xl font-bold mb-6">Import Application Map</h1>
      <p className="text-muted-foreground mb-6">
        Import an application map to create a new application with all its components,
        groups, variables, commands, dependencies, and links.
      </p>

      <div className="grid gap-6">
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Upload className="h-5 w-5" />
              Import Configuration
            </CardTitle>
            <CardDescription>
              Choose your import format and upload a file or paste content directly
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

            <Tabs value={activeTab} onValueChange={(v) => setActiveTab(v as 'json' | 'yaml')}>
              <TabsList className="grid w-full grid-cols-2">
                <TabsTrigger value="json" className="flex items-center gap-2">
                  <FileJson className="h-4 w-4" />
                  JSON (v4 Native)
                </TabsTrigger>
                <TabsTrigger value="yaml" className="flex items-center gap-2">
                  <FileCode className="h-4 w-4" />
                  YAML (Legacy v3)
                </TabsTrigger>
              </TabsList>

              <TabsContent value="json" className="space-y-4 mt-4">
                <div>
                  <label className="block text-sm font-medium mb-1">JSON File</label>
                  <input
                    type="file"
                    accept=".json"
                    onChange={handleFileUpload}
                    className="block w-full text-sm file:mr-4 file:py-2 file:px-4 file:rounded-md file:border-0 file:text-sm file:font-medium file:bg-primary file:text-primary-foreground hover:file:bg-primary/90"
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium mb-1">
                    Or paste JSON content
                  </label>
                  <textarea
                    value={json}
                    onChange={(e) => setJson(e.target.value)}
                    placeholder={`{
  "format_version": "4.0",
  "application": {
    "name": "My Application",
    "components": [...]
  }
}`}
                    className="w-full h-64 px-3 py-2 border rounded-md bg-background text-sm font-mono"
                  />
                </div>
                <p className="text-xs text-muted-foreground">
                  Use JSON v4 format for full feature support including custom commands, links, and all command types.
                  Export an existing application to see the format.
                </p>
              </TabsContent>

              <TabsContent value="yaml" className="space-y-4 mt-4">
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
                <p className="text-xs text-muted-foreground">
                  Legacy YAML format from AppControl v3. Actions are mapped to v4 commands automatically.
                </p>
              </TabsContent>
            </Tabs>

            <Button
              onClick={handleImport}
              disabled={!currentContent || !siteId || activeMutation.isPending}
              className="w-full"
            >
              {activeMutation.isPending ? 'Importing...' : 'Import Map'}
            </Button>
          </CardContent>
        </Card>

        {activeMutation.isSuccess && (
          <Card className="border-green-200 bg-green-50 dark:bg-green-950 dark:border-green-800">
            <CardHeader>
              <CardTitle className="flex items-center gap-2 text-green-700 dark:text-green-300">
                <CheckCircle2 className="h-5 w-5" />
                Import Successful
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="grid grid-cols-2 gap-2 text-sm">
                <div><FileText className="inline h-4 w-4 mr-1" />Application: <strong>{activeMutation.data?.application_name}</strong></div>
                <div>Components: <strong>{activeMutation.data?.components_created}</strong></div>
                <div>Groups: <strong>{activeMutation.data?.groups_created}</strong></div>
                <div>Variables: <strong>{activeMutation.data?.variables_created}</strong></div>
                <div>Commands: <strong>{activeMutation.data?.commands_created}</strong></div>
                <div>Dependencies: <strong>{activeMutation.data?.dependencies_created}</strong></div>
                <div>Links: <strong>{activeMutation.data?.links_created}</strong></div>
              </div>
              {activeMutation.data?.warnings && activeMutation.data.warnings.length > 0 && (
                <div className="mt-4">
                  <h4 className="font-medium text-amber-700 dark:text-amber-300 flex items-center gap-1">
                    <AlertTriangle className="h-4 w-4" />
                    Warnings
                  </h4>
                  <ul className="text-sm text-amber-600 dark:text-amber-400 list-disc list-inside mt-1">
                    {activeMutation.data.warnings.map((w: string, i: number) => (
                      <li key={i}>{w}</li>
                    ))}
                  </ul>
                </div>
              )}
            </CardContent>
          </Card>
        )}

        {activeMutation.isError && (
          <Card className="border-red-200 bg-red-50 dark:bg-red-950 dark:border-red-800">
            <CardHeader>
              <CardTitle className="flex items-center gap-2 text-red-700 dark:text-red-300">
                <AlertTriangle className="h-5 w-5" />
                Import Failed
              </CardTitle>
            </CardHeader>
            <CardContent>
              <p className="text-sm text-red-600 dark:text-red-400">
                {activeMutation.error?.message || 'An error occurred during import'}
              </p>
            </CardContent>
          </Card>
        )}
      </div>
    </div>
  );
}
