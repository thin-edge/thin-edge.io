{{/*
Expand the name of the chart.
*/}}
{{- define "tedge.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
We truncate at 63 chars because some Kubernetes name fields are limited to this (by the DNS naming spec).
If release name contains chart name it will be used as a full name.
*/}}
{{- define "tedge.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "tedge.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "tedge.labels" -}}
helm.sh/chart: {{ include "tedge.chart" . }}
{{ include "tedge.selectorLabels" . }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{/*
Selector labels
*/}}
{{- define "tedge.selectorLabels" -}}
app.kubernetes.io/name: {{ include "tedge.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Name of the chart-managed Secret holding the Cumulocity one-time password.
*/}}
{{- define "tedge.otpSecretName" -}}
tedge-cert-otp
{{- end }}

{{/*
Names and hostPaths for the agent-data and certificate volumes.

PersistentVolumes are cluster-scoped, and the hostPath backing them is a shared
host directory, so both must be unique per install — otherwise a second release
in another namespace collides on the PV name and, worse, shares the same host
files (risking corruption). These default to namespace-scoped values, matching
the "one namespace per device" model; for the documented namespace "tedge" they
resolve to the historical fixed names (tedge-pv, /data/tedge, ...), so existing
installs are unchanged. Any of them can still be overridden via values.
*/}}
{{- define "tedge.persistence.claimName" -}}
{{- .Values.persistence.claimName | default (printf "%s-pvc" .Release.Namespace) }}
{{- end }}
{{- define "tedge.persistence.pvName" -}}
{{- .Values.persistence.pvName | default (printf "%s-pv" .Release.Namespace) }}
{{- end }}
{{- define "tedge.persistence.hostPath" -}}
{{- .Values.persistence.hostPath | default (printf "/data/%s" .Release.Namespace) }}
{{- end }}
{{- define "tedge.certs.claimName" -}}
{{- .Values.certs.persistentVolumeClaim | default (printf "%s-certs-pvc" .Release.Namespace) }}
{{- end }}
{{- define "tedge.certs.pvName" -}}
{{- .Values.certs.pvName | default (printf "%s-certs-pv" .Release.Namespace) }}
{{- end }}
{{- define "tedge.certs.hostPath" -}}
{{- .Values.certs.hostPath | default (printf "/data/%s-certs" .Release.Namespace) }}
{{- end }}

{{/*
Create the name of the service account to use
*/}}
{{- define "tedge.serviceAccountName" -}}
{{- if .Values.serviceAccount.create }}
{{- default (include "tedge.fullname" .) .Values.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.serviceAccount.name }}
{{- end }}
{{- end }}
