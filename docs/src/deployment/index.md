# Cloud Deployment

Ember kann auf verschiedene Arten in der Cloud bereitgestellt werden:

- **Docker** - Einfaches Container-Deployment
- **Kubernetes** - Skalierbare Orchestrierung
- **Helm** - Paketierte Kubernetes-Deployments
- **Terraform** - Infrastructure as Code

## Schnellstart

### Docker

```bash
# Ember API starten
docker run -d \
  -p 8080:8080 \
  -e OPENAI_API_KEY=$OPENAI_API_KEY \
  ghcr.io/ember-ai/ember:latest serve

# Mit Web UI
docker-compose up -d
```

### Kubernetes

```bash
# Namespace erstellen
kubectl apply -f deploy/kubernetes/namespace.yaml

# Konfiguration anwenden
kubectl apply -f deploy/kubernetes/

# Status pruefen
kubectl get pods -n ember
```

### Helm

```bash
# Repository hinzufuegen (zukuenftig)
helm repo add ember https://charts.ember.dev

# Installieren
helm install ember ember/ember \
  --namespace ember \
  --create-namespace \
  --set secrets.openaiApiKey=$OPENAI_API_KEY

# Oder aus lokalen Dateien
helm install ember ./deploy/helm/ember \
  --namespace ember \
  --create-namespace \
  -f my-values.yaml
```

### Terraform (AWS)

```bash
cd deploy/terraform/aws

# Initialisieren
terraform init

# Plan erstellen
terraform plan -var="domain=ember.example.com"

# Anwenden
terraform apply -var="domain=ember.example.com"
```

## Deployment-Optionen

| Option | Vorteile | Nachteile | Empfohlen fuer |
|--------|----------|-----------|----------------|
| Docker | Einfach, schnell | Keine Skalierung | Entwicklung, kleine Teams |
| Kubernetes | Skalierbar, resilient | Komplexitaet | Produktion |
| Helm | Wiederholbar, versioniert | Lernkurve | Teams mit K8s-Erfahrung |
| Terraform | IaC, Multi-Cloud | Komplexitaet | Enterprise, Multi-Cloud |

## Naechste Schritte

- [Docker Deployment](./docker.md)
- [Kubernetes Deployment](./kubernetes.md)
- [Helm Chart](./helm.md)
- [Terraform Module](./terraform.md)