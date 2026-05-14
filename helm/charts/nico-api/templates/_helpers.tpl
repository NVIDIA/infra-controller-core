{{/*
Allow the release namespace to be overridden for multi-namespace deployments.
*/}}
{{- define "nico-api.namespace" -}}
{{- default .Release.Namespace .Values.namespaceOverride | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{/*
Expand the name of the chart.
*/}}
{{- define "nico-api.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "nico-api.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "nico-api.labels" -}}
helm.sh/chart: {{ include "nico-api.chart" . }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/part-of: site-controller
app.kubernetes.io/name: nico-api
app.kubernetes.io/component: api
{{- end }}

{{/*
Selector labels
*/}}
{{- define "nico-api.selectorLabels" -}}
app.kubernetes.io/name: nico-api
app.kubernetes.io/component: api
{{- end }}

{{/*
Global image reference
*/}}
{{- define "nico-api.image" -}}
{{ .Values.global.image.repository }}:{{ .Values.global.image.tag }}
{{- end }}

{{/*
Certificate spec
Usage: {{ include "nico-api.certificateSpec" (dict "name" "nico-api-certificate" "cert" .Values.certificate "global" .Values.global "namespace" (include "nico-api.namespace" .)) }}
*/}}
{{- define "nico-api.certificateSpec" -}}
duration: {{ .global.certificate.duration }}
renewBefore: {{ .global.certificate.renewBefore }}
commonName: {{ printf "%s.%s.svc.cluster.local" .cert.serviceName .namespace }}
dnsNames:
  - {{ printf "%s.%s.svc.cluster.local" .cert.serviceName .namespace }}
{{- if not (eq (toString (.cert.includeShortDnsName | default true)) "false") }}
  - {{ printf "%s.%s" .cert.serviceName .namespace }}
{{- end }}
{{- range .cert.extraDnsNames | default list }}
  - {{ . }}
{{- end }}
uris:
  - {{ printf "spiffe://%s/%s/sa/%s" .global.spiffe.trustDomain .namespace .cert.serviceName }}
{{- range .cert.extraUris | default list }}
  - {{ . }}
{{- end }}
privateKey:
  algorithm: {{ .global.certificate.privateKey.algorithm }}
  size: {{ .global.certificate.privateKey.size }}
issuerRef:
  kind: {{ .global.certificate.issuerRef.kind }}
  name: {{ .global.certificate.issuerRef.name }}
  group: {{ .global.certificate.issuerRef.group }}
secretName: {{ .name }}
{{- end }}

{{/*
Service monitor spec
Usage: {{ include "nico-api.serviceMonitorSpec" (dict "name" "nico-api" "port" "http" "monitor" .Values.serviceMonitor "namespace" "nico-system") }}
*/}}
{{- define "nico-api.serviceMonitorSpec" -}}
endpoints:
  - honorLabels: false
    interval: {{ .monitor.interval }}
    port: {{ .port }}
    scheme: http
    scrapeTimeout: {{ .monitor.scrapeTimeout }}
namespaceSelector:
  matchNames:
    - {{ .namespace }}
selector:
  matchLabels:
    app.kubernetes.io/metrics: {{ .name }}
{{- end }}
