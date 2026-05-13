#!/usr/bin/env bash
# =============================================================================
# health-check.sh
#
# Validates the health of a carbide (Infra Controller) Kubernetes deployment.
# Requires: kubectl, curl (inside pods), openssl (inside carbide-api pod)
#
# Usage:
#   KUBECONFIG=/path/to/kubeconfig ./health-check.sh
#
# All namespaces are auto-detected from cluster resources. Override via env:
#   CARBIDE_NS, VAULT_NS, POSTGRES_NS, CERT_MANAGER_NS, ESO_NS, METALLB_NS
# =============================================================================
set -uo pipefail

# --------------------------------------------------------------------------
# Output helpers
# --------------------------------------------------------------------------
if [ -t 1 ]; then
  _BOLD=$(tput bold 2>/dev/null || printf '')
  _RESET=$(tput sgr0 2>/dev/null || printf '')
  _GREEN=$(tput setaf 2 2>/dev/null || printf '')
  _RED=$(tput setaf 1 2>/dev/null || printf '')
  _YELLOW=$(tput setaf 3 2>/dev/null || printf '')
  _CYAN=$(tput setaf 6 2>/dev/null || printf '')
  _DIM=$(tput dim 2>/dev/null || printf '')
else
  _BOLD=''; _RESET=''; _GREEN=''; _RED=''; _YELLOW=''; _CYAN=''; _DIM=''
fi

PASS=0; FAIL=0; WARN=0; SKIP=0

pass()    { printf "%s  ✓ PASS%s  %s\n"   "${_GREEN}"  "${_RESET}" "$*"; (( PASS++ )) || true; }
fail()    { printf "%s  ✗ FAIL%s  %s\n"   "${_RED}"    "${_RESET}" "$*"; (( FAIL++ )) || true; }
warn()    { printf "%s  ⚠ WARN%s  %s\n"   "${_YELLOW}" "${_RESET}" "$*"; (( WARN++ )) || true; }
skip()    { printf "%s  − SKIP%s  %s\n"   "${_DIM}"    "${_RESET}" "$*"; (( SKIP++ )) || true; }
section() { printf "\n%s══ %s ══%s\n" "${_BOLD}${_CYAN}" "$*" "${_RESET}"; }

# kubectl wrapper: suppresses errors (tests handle their own messaging)
kc() { kubectl "$@" 2>/dev/null; }

_pod_has_command() {
  local ns="$1" pod="$2" container="$3" tool="$4"
  if [[ -n "${container}" ]]; then
    kubectl exec -n "${ns}" "${pod}" -c "${container}" -- \
      sh -c "command -v ${tool} >/dev/null 2>&1" &>/dev/null
  else
    kubectl exec -n "${ns}" "${pod}" -- \
      sh -c "command -v ${tool} >/dev/null 2>&1" &>/dev/null
  fi
}

# --------------------------------------------------------------------------
# Preflight: kubectl accessible
# --------------------------------------------------------------------------
section "Preflight"
if ! kubectl cluster-info &>/dev/null; then
  printf "\n%s  FATAL%s  kubectl cannot reach the cluster. Set KUBECONFIG and retry.\n\n" "${_RED}${_BOLD}" "${_RESET}"
  exit 1
fi
pass "kubectl: cluster reachable"

# --------------------------------------------------------------------------
# Namespace auto-detection
# --------------------------------------------------------------------------
section "Namespace Detection"

# carbide namespace: find the namespace containing vault-cluster-info
if [[ -z "${CARBIDE_NS:-}" ]]; then
  CARBIDE_NS=$(kubectl get configmap vault-cluster-info -A \
    -o jsonpath='{.items[0].metadata.namespace}' 2>/dev/null || true)
  CARBIDE_NS="${CARBIDE_NS:-forge-system}"
fi

# vault namespace: parse from VAULT_SERVICE in vault-cluster-info
# e.g. https://vault.vault.svc.cluster.local:8200 → vault
if [[ -z "${VAULT_NS:-}" ]]; then
  _VAULT_SVC=$(kc get configmap -n "${CARBIDE_NS}" vault-cluster-info \
    -o jsonpath='{.data.VAULT_SERVICE}' || true)
  VAULT_NS=$(printf '%s' "${_VAULT_SVC}" | sed 's|https\?://||' | cut -d: -f1 | cut -d. -f2)
  VAULT_NS="${VAULT_NS:-vault}"
fi
VAULT_ADDR=$(kc get configmap -n "${CARBIDE_NS}" vault-cluster-info \
  -o jsonpath='{.data.VAULT_SERVICE}' 2>/dev/null || \
  printf 'https://vault.%s.svc.cluster.local:8200' "${VAULT_NS}")

# postgres namespace: parse from DB_HOST in database configmap
# e.g. forge-pg-cluster.postgres.svc.cluster.local → postgres
if [[ -z "${POSTGRES_NS:-}" ]]; then
  _DB_HOST=$(kc get configmap -n "${CARBIDE_NS}" forge-system-carbide-database-config \
    -o jsonpath='{.data.DB_HOST}' || true)
  POSTGRES_NS=$(printf '%s' "${_DB_HOST}" | cut -d. -f2)
  POSTGRES_NS="${POSTGRES_NS:-postgres}"
fi

# cert-manager, ESO, MetalLB: discover by known deployment names
if [[ -z "${CERT_MANAGER_NS:-}" ]]; then
  CERT_MANAGER_NS=$(kubectl get deployment cert-manager -A \
    -o jsonpath='{.items[0].metadata.namespace}' 2>/dev/null || printf 'cert-manager')
fi
if [[ -z "${ESO_NS:-}" ]]; then
  ESO_NS=$(kubectl get deployment external-secrets -A \
    -o jsonpath='{.items[0].metadata.namespace}' 2>/dev/null || printf 'external-secrets')
fi
if [[ -z "${METALLB_NS:-}" ]]; then
  METALLB_NS=$(kubectl get deployment metallb-controller -A \
    -o jsonpath='{.items[0].metadata.namespace}' 2>/dev/null || printf 'metallb-system')
fi

printf "  %-26s %s\n" "carbide namespace:"     "${CARBIDE_NS}"
printf "  %-26s %s\n" "vault namespace:"       "${VAULT_NS}"
printf "  %-26s %s\n" "vault address:"         "${VAULT_ADDR}"
printf "  %-26s %s\n" "postgres namespace:"    "${POSTGRES_NS}"
printf "  %-26s %s\n" "cert-manager ns:"       "${CERT_MANAGER_NS}"
printf "  %-26s %s\n" "external-secrets ns:"   "${ESO_NS}"
printf "  %-26s %s\n" "metallb ns:"            "${METALLB_NS}"

# --------------------------------------------------------------------------
# Test helpers
# --------------------------------------------------------------------------

_check_deployment() {
  local ns="$1" name="$2"
  local desired ready
  desired=$(kc get deployment -n "${ns}" "${name}" -o jsonpath='{.spec.replicas}')
  ready=$(kc get deployment -n "${ns}" "${name}" -o jsonpath='{.status.readyReplicas}')
  desired="${desired:-0}"; ready="${ready:-0}"
  if [[ "${desired}" -gt 0 && "${ready}" -ge "${desired}" ]]; then
    pass "deployment/${name}: ${ready}/${desired} ready"
  else
    fail "deployment/${name}: ${ready}/${desired} ready"
  fi
}

_check_statefulset() {
  local ns="$1" name="$2"
  local desired ready
  desired=$(kc get statefulset -n "${ns}" "${name}" -o jsonpath='{.spec.replicas}')
  ready=$(kc get statefulset -n "${ns}" "${name}" -o jsonpath='{.status.readyReplicas}')
  desired="${desired:-0}"; ready="${ready:-0}"
  if [[ "${desired}" -gt 0 && "${ready}" -ge "${desired}" ]]; then
    pass "statefulset/${name}: ${ready}/${desired} ready"
  else
    fail "statefulset/${name}: ${ready}/${desired} ready"
  fi
}

_check_job_complete() {
  local ns="$1" name="$2"
  local status
  status=$(kc get job -n "${ns}" "${name}" \
    -o jsonpath='{.status.conditions[?(@.type=="Complete")].status}')
  if [[ "${status}" == "True" ]]; then
    pass "job/${name}: Complete"
  else
    fail "job/${name}: not Complete (status=${status:-unknown})"
  fi
}

_check_secret_exists() {
  local ns="$1" name="$2"
  if kc get secret -n "${ns}" "${name}" &>/dev/null; then
    pass "secret/${name}: exists"
  else
    fail "secret/${name}: not found in ${ns}"
  fi
}

_check_secret_key() {
  local ns="$1" name="$2" key="$3"
  local val
  val=$(kc get secret -n "${ns}" "${name}" -o jsonpath="{.data.${key}}" 2>/dev/null || true)
  if [[ -n "${val}" ]]; then
    pass "secret/${name}[${key}]: present"
  else
    fail "secret/${name}[${key}]: missing or empty"
  fi
}

_check_configmap_key() {
  local ns="$1" name="$2" key="$3"
  local val
  val=$(kc get configmap -n "${ns}" "${name}" -o jsonpath="{.data.${key}}" 2>/dev/null || true)
  if [[ -n "${val}" ]]; then
    pass "configmap/${name}[${key}]: present"
  else
    fail "configmap/${name}[${key}]: missing or empty"
  fi
}

# HTTP check: exec curl from inside a running pod
# _http_check DESC NS LABEL_SELECTOR URL EXPECTED_HTTP_CODE
_http_check() {
  local desc="$1" ns="$2" selector="$3" url="$4" expected="$5"
  local pod code
  pod=$(kc get pod -n "${ns}" -l "${selector}" \
    --field-selector=status.phase=Running \
    -o jsonpath='{.items[0].metadata.name}')
  if [[ -z "${pod}" ]]; then
    fail "${desc}: no running pod found (selector: ${selector})"
    return
  fi
  if ! _pod_has_command "${ns}" "${pod}" "" curl; then
    skip "${desc}: curl not available in pod/${pod}"
    return
  fi
  code=$(kubectl exec -n "${ns}" "${pod}" -- \
    curl -sf -o /dev/null -w '%{http_code}' --max-time 5 "${url}" 2>/dev/null || printf '000')
  if [[ "${code}" == "${expected}" ]]; then
    pass "${desc}: HTTP ${code}"
  elif [[ "${expected}" == "2xx" && "${code}" =~ ^2 ]]; then
    pass "${desc}: HTTP ${code}"
  else
    fail "${desc}: HTTP ${code} (expected ${expected})"
  fi
}

# --------------------------------------------------------------------------
# 1. Supporting infrastructure
# --------------------------------------------------------------------------
section "cert-manager"
_check_deployment "${CERT_MANAGER_NS}" cert-manager
_check_deployment "${CERT_MANAGER_NS}" cert-manager-cainjector
_check_deployment "${CERT_MANAGER_NS}" cert-manager-webhook

section "External Secrets Operator"
_check_deployment "${ESO_NS}" external-secrets
_check_deployment "${ESO_NS}" external-secrets-cert-controller
_check_deployment "${ESO_NS}" external-secrets-webhook

section "MetalLB"
_check_deployment "${METALLB_NS}" metallb-controller
_desired=$(kc get daemonset -n "${METALLB_NS}" metallb-speaker \
  -o jsonpath='{.status.desiredNumberScheduled}' || printf '0')
_ready=$(kc get daemonset -n "${METALLB_NS}" metallb-speaker \
  -o jsonpath='{.status.numberReady}' || printf '0')
_desired="${_desired:-0}"; _ready="${_ready:-0}"
if [[ "${_desired}" -gt 0 && "${_ready}" -ge "${_desired}" ]]; then
  pass "daemonset/metallb-speaker: ${_ready}/${_desired} ready"
else
  fail "daemonset/metallb-speaker: ${_ready:-0}/${_desired} ready"
fi

# --------------------------------------------------------------------------
# 2. Vault
# --------------------------------------------------------------------------
section "Vault"
_check_statefulset "${VAULT_NS}" vault

_vault_json_field() {
  printf '%s' "$1" | grep -o "\"$2\":[^,}]*" | cut -d: -f2 | tr -d ' "'
}

_VAULT_REPLICAS=$(kc get statefulset -n "${VAULT_NS}" vault \
  -o jsonpath='{.spec.replicas}' || printf '0')
_VAULT_REPLICAS="${_VAULT_REPLICAS:-0}"
[[ "${_VAULT_REPLICAS}" =~ ^[0-9]+$ ]] || _VAULT_REPLICAS=0
_VAULT_HA=false

if [[ "${_VAULT_REPLICAS}" -le 0 ]]; then
  fail "vault: no replicas found"
else
  for _IDX in $(seq 0 $(( _VAULT_REPLICAS - 1 ))); do
    _VAULT_POD="vault-${_IDX}"
    _VAULT_JSON=$(kubectl exec -n "${VAULT_NS}" "${_VAULT_POD}" -c vault -- \
      vault status -address="${VAULT_ADDR}" -tls-skip-verify -format=json 2>/dev/null || printf '{}')

    _VAULT_INIT=$(_vault_json_field "${_VAULT_JSON}" initialized)
    _VAULT_SEALED=$(_vault_json_field "${_VAULT_JSON}" sealed)
    _VAULT_POD_HA=$(_vault_json_field "${_VAULT_JSON}" ha_enabled)

    if [[ "${_VAULT_INIT}" == "true" ]]; then
      pass "${_VAULT_POD}: initialized"
    else
      fail "${_VAULT_POD}: not initialized"
    fi
    if [[ "${_VAULT_SEALED}" == "false" ]]; then
      pass "${_VAULT_POD}: unsealed"
    else
      fail "${_VAULT_POD}: sealed — cluster cannot operate"
    fi
    [[ "${_VAULT_POD_HA}" == "true" ]] && _VAULT_HA=true
  done
fi

if [[ "${_VAULT_HA}" == "true" ]]; then
  pass "vault: HA enabled"
else
  warn "vault: HA not enabled"
fi

# vault-pki-config job completion implies PKI+auth are configured correctly
# (detailed checks would require vault token — covered by job + approle secret checks)

# --------------------------------------------------------------------------
# 3. PostgreSQL
# --------------------------------------------------------------------------
section "PostgreSQL"
_check_statefulset "${POSTGRES_NS}" forge-pg-cluster

_PG_OP_READY=$(kc get deployment -n "${POSTGRES_NS}" postgres-operator \
  -o jsonpath='{.status.readyReplicas}' || printf '0')
if [[ "${_PG_OP_READY:-0}" -ge 1 ]]; then
  pass "deployment/postgres-operator: running"
else
  fail "deployment/postgres-operator: not ready"
fi

_DB_NAME=$(kc get configmap -n "${CARBIDE_NS}" forge-system-carbide-database-config \
  -o jsonpath='{.data.DB_NAME}' 2>/dev/null || printf 'forge_system_carbide')
_DB_EXISTS=$(kubectl exec -n "${POSTGRES_NS}" forge-pg-cluster-0 -c postgres -- \
  psql -U postgres -lqt 2>/dev/null | grep -c "${_DB_NAME}" || printf '0')
if [[ "${_DB_EXISTS}" -ge 1 ]]; then
  pass "postgres: database '${_DB_NAME}' exists"
else
  fail "postgres: database '${_DB_NAME}' not found"
fi

# --------------------------------------------------------------------------
# 4. carbide core pods
# --------------------------------------------------------------------------
section "carbide Pods"
_check_deployment  "${CARBIDE_NS}" carbide-api
_check_deployment  "${CARBIDE_NS}" carbide-dhcp
_check_statefulset "${CARBIDE_NS}" carbide-dns
_check_deployment  "${CARBIDE_NS}" carbide-pxe

# Optional pods: warn if the deployment doesn't exist, fail if it exists but isn't ready
for _OPT_DEP in carbide-hardware-health carbide-ssh-console-rs carbide-dsx-exchange-consumer; do
  if kc get deployment -n "${CARBIDE_NS}" "${_OPT_DEP}" &>/dev/null; then
    _check_deployment "${CARBIDE_NS}" "${_OPT_DEP}"
  else
    skip "${_OPT_DEP}: not deployed"
  fi
done

section "carbide Jobs"
_check_job_complete "${CARBIDE_NS}" vault-pki-config

# Migration job: find by label (name includes a random suffix)
_MIG_JOB=$(kc get jobs -n "${CARBIDE_NS}" -l 'app.kubernetes.io/name=carbide-api-migrate' \
  --sort-by=.metadata.creationTimestamp \
  -o jsonpath='{.items[-1:].metadata.name}' 2>/dev/null | awk '{print $NF}' | head -1 || true)
if [[ -z "${_MIG_JOB}" ]]; then
  # fallback: name contains 'migrate'
  _MIG_JOB=$(kc get jobs -n "${CARBIDE_NS}" --no-headers 2>/dev/null | \
    awk '/carbide-api-migrate/{print $1}' | tail -1 || true)
fi
if [[ -n "${_MIG_JOB}" ]]; then
  _check_job_complete "${CARBIDE_NS}" "${_MIG_JOB}"
else
  fail "carbide-api-migrate: job not found"
fi

# --------------------------------------------------------------------------
# 5. PKI — ClusterIssuers + Certificates
# --------------------------------------------------------------------------
section "ClusterIssuers"
for _ISSUER in selfsigned-bootstrap site-issuer vault-forge-issuer; do
  _READY=$(kc get clusterissuer "${_ISSUER}" \
    -o jsonpath='{.status.conditions[?(@.type=="Ready")].status}')
  if [[ "${_READY}" == "True" ]]; then
    pass "clusterissuer/${_ISSUER}: Ready"
  else
    fail "clusterissuer/${_ISSUER}: not Ready (${_READY:-missing})"
  fi
done

section "Certificates"
_NOW_EPOCH=$(date +%s)
while IFS= read -r _CERT; do
  [[ -z "${_CERT}" ]] && continue
  _READY=$(kc get certificate -n "${CARBIDE_NS}" "${_CERT}" \
    -o jsonpath='{.status.conditions[?(@.type=="Ready")].status}')
  if [[ "${_READY}" != "True" ]]; then
    fail "certificate/${_CERT}: not Ready"
    continue
  fi
  _EXPIRY=$(kc get certificate -n "${CARBIDE_NS}" "${_CERT}" \
    -o jsonpath='{.status.notAfter}')
  if [[ -n "${_EXPIRY}" ]]; then
    _EXP_EPOCH=$(date -d "${_EXPIRY}" +%s 2>/dev/null || \
                 date -jf "%Y-%m-%dT%H:%M:%SZ" "${_EXPIRY}" +%s 2>/dev/null || printf '0')
    _DAYS=$(( (_EXP_EPOCH - _NOW_EPOCH) / 86400 ))
    if [[ "${_DAYS}" -lt 7 ]]; then
      warn "certificate/${_CERT}: Ready but expires in ${_DAYS} days (${_EXPIRY})"
    else
      pass "certificate/${_CERT}: Ready, expires in ${_DAYS} days"
    fi
  else
    pass "certificate/${_CERT}: Ready"
  fi
done < <(kc get certificates -n "${CARBIDE_NS}" \
  --no-headers -o custom-columns=NAME:.metadata.name 2>/dev/null)

# --------------------------------------------------------------------------
# 6. Required secrets & ConfigMaps
# --------------------------------------------------------------------------
section "Secrets"
_check_secret_exists "${CARBIDE_NS}" carbide-vault-token
_check_secret_exists "${CARBIDE_NS}" carbide-vault-approle-tokens
_check_secret_key    "${CARBIDE_NS}" carbide-vault-approle-tokens VAULT_ROLE_ID
_check_secret_key    "${CARBIDE_NS}" carbide-vault-approle-tokens VAULT_SECRET_ID
_check_secret_exists "${CARBIDE_NS}" forge-roots
_check_secret_exists "${CARBIDE_NS}" ssh-host-key
_check_secret_exists "${CARBIDE_NS}" "forge-system.carbide.forge-pg-cluster.credentials"
for _CERT_S in carbide-api-certificate carbide-pxe-certificate \
               carbide-dhcp-certificate carbide-dns-certificate; do
  _check_secret_exists "${CARBIDE_NS}" "${_CERT_S}"
done

section "ConfigMaps"
_check_configmap_key "${CARBIDE_NS}" vault-cluster-info                   VAULT_SERVICE
_check_configmap_key "${CARBIDE_NS}" vault-cluster-info                   FORGE_VAULT_PKI_MOUNT
_check_configmap_key "${CARBIDE_NS}" forge-system-carbide-database-config DB_HOST
_check_configmap_key "${CARBIDE_NS}" forge-system-carbide-database-config DB_NAME
_check_configmap_key "${CARBIDE_NS}" forge-system-carbide-database-config DB_PORT

for _CM in carbide-web-api-hostname carbide-api-site-config-files \
           carbide-api-config-files carbide-dhcp-config carbide-dns; do
  if kc get configmap -n "${CARBIDE_NS}" "${_CM}" &>/dev/null; then
    pass "configmap/${_CM}: exists"
  else
    fail "configmap/${_CM}: not found"
  fi
done

# --------------------------------------------------------------------------
# 7. External Secrets Operator
# --------------------------------------------------------------------------
section "ClusterSecretStores"
while IFS= read -r _CSS; do
  [[ -z "${_CSS}" ]] && continue
  _READY=$(kc get clustersecretstore "${_CSS}" \
    -o jsonpath='{.status.conditions[?(@.type=="Ready")].status}')
  if [[ "${_READY}" == "True" ]]; then
    pass "clustersecretstore/${_CSS}: Valid"
  else
    fail "clustersecretstore/${_CSS}: not Valid (${_READY:-missing})"
  fi
done < <(kc get clustersecretstore --no-headers \
  -o custom-columns=NAME:.metadata.name 2>/dev/null)

section "ExternalSecrets"
while IFS= read -r _LINE; do
  [[ -z "${_LINE}" ]] && continue
  _ES_NAME=$(printf '%s' "${_LINE}" | awk '{print $1}')
  _ES_NS=$(printf '%s' "${_LINE}" | awk '{print $2}')
  _REASON=$(kc get externalsecret -n "${_ES_NS}" "${_ES_NAME}" \
    -o jsonpath='{.status.conditions[?(@.type=="Ready")].reason}')
  if [[ "${_REASON}" == "SecretSynced" ]]; then
    pass "externalsecret/${_ES_NAME} (${_ES_NS}): SecretSynced"
  else
    fail "externalsecret/${_ES_NAME} (${_ES_NS}): ${_REASON:-not synced}"
  fi
done < <(kc get externalsecret -A --no-headers \
  -o custom-columns=NAME:.metadata.name,NS:.metadata.namespace 2>/dev/null)

# --------------------------------------------------------------------------
# 8. External Services — LoadBalancer VIPs
# --------------------------------------------------------------------------
section "External Service VIPs (LoadBalancer)"
while IFS= read -r _SVC; do
  [[ -z "${_SVC}" ]] && continue
  _IP=$(kc get svc -n "${CARBIDE_NS}" "${_SVC}" \
    -o jsonpath='{.status.loadBalancer.ingress[0].ip}')
  _PORT=$(kc get svc -n "${CARBIDE_NS}" "${_SVC}" \
    -o jsonpath='{.spec.ports[0].port}')
  if [[ -n "${_IP}" && "${_IP}" != "pending" ]]; then
    pass "svc/${_SVC}: ${_IP}:${_PORT}"
  else
    fail "svc/${_SVC}: no external IP (still pending)"
  fi
done < <(kc get svc -n "${CARBIDE_NS}" --no-headers 2>/dev/null | \
  awk '$2=="LoadBalancer"{print $1}')

# --------------------------------------------------------------------------
# 9. External VIP reachability (from MetalLB speaker — hostNetwork)
# --------------------------------------------------------------------------
# MetalLB speaker pods run with hostNetwork:true, giving them the full host
# routing table including BGP-learned routes. nc -zw2 from a speaker pod
# proves the VIP is routable and the service is accepting connections,
# equivalent to a test from an OOB-network host.
# --------------------------------------------------------------------------
section "External VIP Reachability (via MetalLB speaker)"

_SPEAKER=$(kc get pod -n "${METALLB_NS}" \
  -l "app.kubernetes.io/component=speaker,app.kubernetes.io/name=metallb" \
  --no-headers -o custom-columns=NAME:.metadata.name | head -1)

if [[ -z "${_SPEAKER:-}" ]]; then
  warn "MetalLB speaker pod not found — skipping external reachability tests"
elif ! _pod_has_command "${METALLB_NS}" "${_SPEAKER}" speaker nc; then
  skip "MetalLB speaker pod/${_SPEAKER}: nc not available — skipping external reachability tests"
else
  while IFS= read -r _SVC; do
    [[ -z "${_SVC}" ]] && continue
    _IP=$(kc get svc -n "${CARBIDE_NS}" "${_SVC}" \
      -o jsonpath='{.status.loadBalancer.ingress[0].ip}')
    _PORT=$(kc get svc -n "${CARBIDE_NS}" "${_SVC}" \
      -o jsonpath='{.spec.ports[0].port}')
    _PROTO=$(kc get svc -n "${CARBIDE_NS}" "${_SVC}" \
      -o jsonpath='{.spec.ports[0].protocol}')

    # Skip if no IP assigned yet
    [[ -z "${_IP}" || "${_IP}" == "pending" ]] && continue

    # UDP-only services cannot be tested with TCP nc; DNS UDP is covered by the
    # DNS section, NTP has no reliable probe, DHCP requires a full handshake.
    # Services with TCP+UDP on the same VIP: skip the UDP duplicate (name contains "udp").
    [[ "${_PROTO}" == "UDP" ]] && continue
    printf '%s' "${_SVC}" | grep -q "udp" && continue

    if kubectl exec -n "${METALLB_NS}" "${_SPEAKER}" -c speaker -- \
        nc -zw2 "${_IP}" "${_PORT}" &>/dev/null; then
      pass "svc/${_SVC}: ${_IP}:${_PORT} reachable from host network"
    else
      fail "svc/${_SVC}: ${_IP}:${_PORT} not reachable (BGP route missing or service not listening)"
    fi
  done < <(kc get svc -n "${CARBIDE_NS}" --no-headers 2>/dev/null | \
    awk '$2=="LoadBalancer"{print $1}')
fi

# --------------------------------------------------------------------------
# 10. In-cluster service connectivity
# --------------------------------------------------------------------------
section "In-Cluster Service Connectivity"

_http_check "carbide-api metrics"       "${CARBIDE_NS}" \
  "app.kubernetes.io/name=carbide-api"             "http://localhost:1080/metrics"  "200"
_http_check "carbide-pxe metrics"       "${CARBIDE_NS}" \
  "app.kubernetes.io/name=carbide-pxe"             "http://localhost:8080/metrics"  "200"
_http_check "carbide-dhcp metrics"      "${CARBIDE_NS}" \
  "app.kubernetes.io/name=carbide-dhcp"            "http://localhost:1089/metrics"  "200"

if kc get deployment -n "${CARBIDE_NS}" carbide-hardware-health &>/dev/null; then
  _http_check "carbide-hardware-health" "${CARBIDE_NS}" \
    "app.kubernetes.io/name=carbide-hardware-health" "http://localhost:9009/"       "200"
fi

# TLS: verify carbide-api gRPC port presents a certificate
_API_POD=$(kc get pod -n "${CARBIDE_NS}" -l "app.kubernetes.io/name=carbide-api" \
  --field-selector=status.phase=Running -o jsonpath='{.items[0].metadata.name}')
if [[ -n "${_API_POD:-}" ]]; then
  if ! _pod_has_command "${CARBIDE_NS}" "${_API_POD}" "" openssl; then
    skip "carbide-api gRPC TLS (:1079): openssl not available in pod/${_API_POD}"
  else
    _TLS=$(kubectl exec -n "${CARBIDE_NS}" "${_API_POD}" -- \
      sh -c "echo | openssl s_client \
        -connect carbide-api.${CARBIDE_NS}.svc.cluster.local:1079 \
        -verify_quiet 2>&1 | head -5" 2>/dev/null || true)
    if printf '%s' "${_TLS}" | grep -qiE "CONNECTED|Protocol version|TLSv"; then
      pass "carbide-api gRPC TLS (:1079): TLS handshake succeeded"
    else
      warn "carbide-api gRPC TLS (:1079): could not verify (check CA trust)"
    fi
  fi
fi

# --------------------------------------------------------------------------
# 11. DNS — forge-unbound
# --------------------------------------------------------------------------
section "DNS (forge-unbound)"
_UNBOUND_POD=$(kc get pod -n "${CARBIDE_NS}" -l "app.kubernetes.io/name=unbound" \
  --field-selector=status.phase=Running -o jsonpath='{.items[0].metadata.name}' || true)

if [[ -z "${_UNBOUND_POD:-}" ]]; then
  warn "forge-unbound: no running pod found (check unbound.enabled in values)"
else
  # Verify daemon is running via unbound-control (available in the unbound image)
  if ! _pod_has_command "${CARBIDE_NS}" "${_UNBOUND_POD}" "" unbound-control; then
    skip "forge-unbound: unbound-control not available in pod/${_UNBOUND_POD}"
  else
    _UC=$(kubectl exec -n "${CARBIDE_NS}" "${_UNBOUND_POD}" -- \
      unbound-control status 2>/dev/null | grep -c "is running" || printf '0')
    if [[ "${_UC}" -ge 1 ]]; then
      pass "forge-unbound: daemon running"
    else
      fail "forge-unbound: daemon not running (unbound-control status failed)"
    fi
  fi

  # Resolution tests: use MetalLB speaker's nslookup via the external VIP.
  # This tests the full path (VIP → service → DNS resolution) from the host network.
  _UNBOUND_VIP=$(kc get svc -n "${CARBIDE_NS}" forge-unbound-external \
    -o jsonpath='{.status.loadBalancer.ingress[0].ip}' || true)
  if [[ -z "${_UNBOUND_VIP:-}" ]]; then
    _UNBOUND_VIP=$(kc get svc -n "${CARBIDE_NS}" -l "app.kubernetes.io/name=unbound" \
      --no-headers 2>/dev/null | awk '$2=="LoadBalancer" && $4!="<pending>"{print $4; exit}' || true)
  fi

  if [[ -n "${_SPEAKER:-}" && -n "${_UNBOUND_VIP:-}" ]]; then
    if ! _pod_has_command "${METALLB_NS}" "${_SPEAKER}" speaker nslookup; then
      skip "forge-unbound: nslookup not available in MetalLB speaker pod/${_SPEAKER}"
      continue_dns_tests=false
    else
      continue_dns_tests=true
    fi
  else
    continue_dns_tests=false
  fi

  if [[ "${continue_dns_tests}" == "true" ]]; then
    # Recursive resolution
    _EXT=$(kubectl exec -n "${METALLB_NS}" "${_SPEAKER}" -c speaker -- \
      nslookup example.com "${_UNBOUND_VIP}" 2>/dev/null | grep '^Address' | tail -1 || true)
    if [[ -n "${_EXT:-}" ]]; then
      pass "forge-unbound: recursive resolution (example.com via ${_UNBOUND_VIP})"
    else
      fail "forge-unbound: recursive resolution failed (example.com via ${_UNBOUND_VIP})"
    fi

    # .forge zone — should resolve to the carbide-api external LB IP
    _FORGE=$(kubectl exec -n "${METALLB_NS}" "${_SPEAKER}" -c speaker -- \
      nslookup carbide-api.forge "${_UNBOUND_VIP}" 2>/dev/null | grep '^Address' | tail -1 || true)
    if [[ -n "${_FORGE:-}" ]]; then
      pass "forge-unbound: .forge zone (carbide-api.forge via ${_UNBOUND_VIP})"
    else
      warn "forge-unbound: .forge zone not responding (check local_data.conf in unbound values)"
    fi
  elif [[ -z "${_SPEAKER:-}" || -z "${_UNBOUND_VIP:-}" ]]; then
    warn "forge-unbound: resolution tests skipped (no MetalLB speaker or VIP available)"
  fi
fi

# --------------------------------------------------------------------------
# 12. NTP (carbide-ntp)
# --------------------------------------------------------------------------
section "NTP (carbide-ntp)"
_NTP_TOTAL=$(kc get pods -n "${CARBIDE_NS}" -l "app.kubernetes.io/name=carbide-ntp" \
  --no-headers 2>/dev/null | wc -l | tr -d ' ')
if [[ "${_NTP_TOTAL:-0}" -eq 0 ]]; then
  warn "carbide-ntp: no pods found (may be deployed via kustomize overlay — check deploy/envs/<site>/)"
else
  _NTP_READY=$(kc get pods -n "${CARBIDE_NS}" -l "app.kubernetes.io/name=carbide-ntp" \
    --no-headers 2>/dev/null | awk '/Running/{n++} END{print n+0}')
  if [[ "${_NTP_READY}" -ge "${_NTP_TOTAL}" ]]; then
    pass "carbide-ntp: ${_NTP_READY}/${_NTP_TOTAL} pods Running"
  else
    fail "carbide-ntp: ${_NTP_READY}/${_NTP_TOTAL} pods Running"
  fi
fi

# --------------------------------------------------------------------------
# .forge DNS endpoint reference (from DNS.md)
# --------------------------------------------------------------------------
section ".forge DNS Endpoint Reference"
printf "  %s\n" "The following hostnames must resolve on the OOB management network:"
printf "  %-36s %-6s %s\n" "Hostname" "Port" "Protocol"
printf "  %-36s %-6s %s\n" "--------" "----" "--------"
printf "  %-36s %-6s %s\n" "carbide-api.forge"         "443"  "gRPC/TLS (DPU agents, CLI, PXE, DHCP)"
printf "  %-36s %-6s %s\n" "carbide-pxe.forge"         "80"   "HTTP     (DPU agents — HARDCODED in agent binary)"
printf "  %-36s %-6s %s\n" "carbide-static-pxe.forge"  "80"   "HTTP     (host PXE loader — HARDCODED in boot images)"
printf "  %-36s %-6s %s\n" "carbide-ntp.forge"         "123"  "NTP/UDP  (DPU agents — HARDCODED in agent binary)"
printf "  %-36s %-6s %s\n" "unbound.forge"             "53"   "DNS      (distributed via DHCP option 6)"
printf "  %-36s %-6s %s\n" "otel-receiver.forge"       "443"  "gRPC/TLS (otel-collector sidecars)"
printf "  %-36s %-6s %s\n" "socks.forge"               "1888" "SOCKS5   (DPU extension services — HARDCODED in agent binary)"
printf "\n  %s\n" "Verify with: dig +short <hostname> @<UNBOUND_VIP>"

# --------------------------------------------------------------------------
# Summary
# --------------------------------------------------------------------------
_TOTAL=$(( PASS + FAIL + WARN + SKIP ))
printf "\n%s%s════════════════════════════════════════════════%s\n" "${_BOLD}" "${_CYAN}" "${_RESET}"
if [[ "${FAIL}" -eq 0 ]]; then
  printf "%s%s  ALL CHECKS PASSED%s\n" "${_GREEN}" "${_BOLD}" "${_RESET}"
else
  printf "%s%s  %d CHECK(S) FAILED%s\n" "${_RED}" "${_BOLD}" "${FAIL}" "${_RESET}"
fi
printf "%s  ✓ %-3d passed  ✗ %-3d failed  ⚠ %-3d warnings  − %-3d skipped  (%d total)%s\n" \
  "${_BOLD}" "${PASS}" "${FAIL}" "${WARN}" "${SKIP}" "${_TOTAL}" "${_RESET}"
printf "%s%s════════════════════════════════════════════════%s\n\n" "${_BOLD}" "${_CYAN}" "${_RESET}"

[[ "${FAIL}" -eq 0 ]]
