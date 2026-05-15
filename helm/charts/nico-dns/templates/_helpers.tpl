{{/*
Allow the release namespace to be overridden for multi-namespace deployments.
*/}}
{{- define "nico-dns.namespace" -}}
{{- default .Release.Namespace .Values.namespaceOverride | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{/*
Expand the name of the chart.
*/}}
{{- define "nico-dns.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "nico-dns.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "nico-dns.labels" -}}
helm.sh/chart: {{ include "nico-dns.chart" . }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/part-of: site-controller
app.kubernetes.io/name: nico-dns
app.kubernetes.io/component: dns
{{- end }}

{{/*
Selector labels
*/}}
{{- define "nico-dns.selectorLabels" -}}
app.kubernetes.io/name: nico-dns
app.kubernetes.io/component: dns
{{- end }}

{{/*
Global image reference
*/}}
{{- define "nico-dns.image" -}}
{{ .Values.global.image.repository }}:{{ .Values.global.image.tag }}
{{- end }}

{{/*
Certificate spec
*/}}
{{- define "nico-dns.certificateSpec" -}}
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
