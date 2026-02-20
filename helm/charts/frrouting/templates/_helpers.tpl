{{/*
Expand the name of the chart.
*/}}
{{- define "frrouting.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "frrouting.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "frrouting.labels" -}}
helm.sh/chart: {{ include "frrouting.chart" . }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/name: frrouting
app.kubernetes.io/component: frrouting
{{- end }}

{{/*
Selector labels
*/}}
{{- define "frrouting.selectorLabels" -}}
app.kubernetes.io/name: frrouting
app.kubernetes.io/component: frrouting
{{- end }}
