import ImportWizard from './ImportWizard';

export default function ImportPage() {
  return (
    <div className="container mx-auto p-6 max-w-5xl">
      <div className="mb-6">
        <h1 className="text-2xl font-bold">Import Application</h1>
        <p className="text-muted-foreground">
          Import a map file to create a new application.
        </p>
      </div>
      <ImportWizard />
    </div>
  );
}
