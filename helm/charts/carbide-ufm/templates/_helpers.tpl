{{/*
Expand the name of the chart.
*/}}
{{- define "carbide-ufm.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "carbide-ufm.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "carbide-ufm.labels" -}}
helm.sh/chart: {{ include "carbide-ufm.chart" . }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/part-of: site-controller
app.kubernetes.io/name: ufm-enterprise
app.kubernetes.io/component: ufm
{{- end }}
