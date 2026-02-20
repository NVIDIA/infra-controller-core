{{/*
Expand the name of the chart.
*/}}
{{- define "carbide-nvpasswd-unexpirer.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "carbide-nvpasswd-unexpirer.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "carbide-nvpasswd-unexpirer.labels" -}}
helm.sh/chart: {{ include "carbide-nvpasswd-unexpirer.chart" . }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/part-of: site-controller
app.kubernetes.io/name: nvpasswd-unexpirer
app.kubernetes.io/component: nvpasswd-unexpirer
{{- end }}

{{/*
Selector labels
*/}}
{{- define "carbide-nvpasswd-unexpirer.selectorLabels" -}}
app.kubernetes.io/name: nvpasswd-unexpirer
app.kubernetes.io/component: nvpasswd-unexpirer
{{- end }}
