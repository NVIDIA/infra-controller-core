{{/*
Expand the name of the chart.
*/}}
{{- define "carbide-nginx.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "carbide-nginx.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "carbide-nginx.labels" -}}
helm.sh/chart: {{ include "carbide-nginx.chart" . }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/part-of: site-controller
app.kubernetes.io/name: nginx
app.kubernetes.io/component: nginx
{{- end }}

{{/*
Selector labels
*/}}
{{- define "carbide-nginx.selectorLabels" -}}
app.kubernetes.io/name: nginx
app.kubernetes.io/component: nginx
{{- end }}

{{/*
Global image reference
*/}}
{{- define "carbide-nginx.image" -}}
{{ .Values.global.image.repository }}:{{ .Values.global.image.tag }}
{{- end }}

{{/*
Certificate spec
*/}}
{{- define "carbide-nginx.certificateSpec" -}}
duration: {{ .global.certificate.duration }}
renewBefore: {{ .global.certificate.renewBefore }}
dnsNames:
  - {{ printf "%s.%s.svc.cluster.local" .cert.serviceName .global.namespace }}
  - {{ printf "%s.%s" .cert.serviceName .global.namespace }}
{{- range .cert.extraDnsNames | default list }}
  - {{ . }}
{{- end }}
uris:
  - {{ printf "spiffe://%s/%s/sa/%s" .global.spiffe.trustDomain .global.namespace .cert.serviceName }}
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
