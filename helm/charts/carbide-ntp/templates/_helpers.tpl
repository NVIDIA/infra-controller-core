{{/*
Expand the name of the chart.
*/}}
{{- define "carbide-ntp.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "carbide-ntp.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "carbide-ntp.labels" -}}
helm.sh/chart: {{ include "carbide-ntp.chart" . }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/part-of: site-controller
app.kubernetes.io/name: carbide-ntp
app.kubernetes.io/component: ntp
{{- end }}

{{/*
Selector labels
*/}}
{{- define "carbide-ntp.selectorLabels" -}}
app.kubernetes.io/name: carbide-ntp
app.kubernetes.io/component: ntp
{{- end }}

{{/*
Generate NTP_SERVERS value: external servers + peer FQDNs using global.namespace.
Replaces hardcoded forge-system in peer discovery.
*/}}
{{- define "carbide-ntp.ntpServers" -}}
{{- $ns := .Values.global.namespace -}}
{{- $external := splitList "," .Values.env.NTP_EXTERNAL_SERVERS -}}
{{- $peers := list -}}
{{- $replicas := int .Values.replicas -}}
{{- range $i := until $replicas -}}
  {{- $peers = append $peers (printf "carbide-ntp-%d.carbide-ntp.%s.svc.cluster.local" $i $ns) -}}
{{- end -}}
{{ join "," (concat $external $peers) }}
{{- end }}
