# CLAUDE.md - helm/

## Purpose
Helm chart for deploying AppControl on Kubernetes/OpenShift.

## Chart Structure
```
helm/appcontrol/
├── Chart.yaml
├── values.yaml
├── templates/
│   ├── backend-deployment.yaml
│   ├── backend-service.yaml
│   ├── frontend-deployment.yaml
│   ├── frontend-service.yaml
│   ├── postgresql-statefulset.yaml    # or use external operator
│   ├── redis-deployment.yaml
│   ├── gateway-deployment.yaml
│   ├── ingress.yaml                   # K8s Ingress
│   ├── route.yaml                     # OpenShift Route (conditional)
│   ├── configmap.yaml
│   ├── secret.yaml
│   ├── cronjob-partition.yaml         # Monthly partition maintenance
│   └── _helpers.tpl
```

## OpenShift Compatibility
- All containers run as non-root (runAsNonRoot: true)
- No privileged containers
- Use `{{ if .Values.openshift.enabled }}` for Route vs Ingress
- SecurityContextConstraints: use "restricted" SCC
- Image streams optional (for internal registry)

## Key values.yaml Settings
```yaml
backend:
  replicas: 2
  resources: { requests: { cpu: 1, memory: 2Gi }, limits: { cpu: 2, memory: 4Gi } }
frontend:
  replicas: 2
  resources: { requests: { cpu: 250m, memory: 128Mi }, limits: { cpu: 500m, memory: 256Mi } }
postgresql:
  enabled: true  # false if using external DB
  resources: { requests: { cpu: 2, memory: 4Gi }, limits: { cpu: 4, memory: 8Gi } }
  storage: 100Gi
redis:
  enabled: true
gateway:
  zones: [{ name: PRD, replicas: 1 }, { name: DMZ, replicas: 1 }]
openshift:
  enabled: false
```
