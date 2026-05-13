#!/usr/bin/env bash
# SPDX-FileCopyrightText: Copyright (c) 2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
# SPDX-License-Identifier: Apache-2.0
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
# http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# =============================================================================
# setup.sh — install the NVIDIA Infra Controller prerequisite stack
#
# Tool requirements:
#   helmfile, helm, kubectl, jq, ssh-keygen
#
# Required environment:
#   KUBECONFIG            Optional only if the current kubectl context already
#                         points at the target cluster.
#   NCX_IMAGE_REGISTRY    Required unless both --skip-core and --skip-rest are
#                         used. Registry/repository prefix for Infra Controller images,
#                         without http(s)://. Example: registry.example.com/ncx
#   NCX_CORE_IMAGE_TAG    Required unless --skip-core is used.
#                         Infra Controller Core tag. Example: v2025.12.30
#   NCX_REST_IMAGE_TAG    Required unless --skip-rest is used.
#                         Infra Controller REST tag. Example: v1.0.4
#
# Optional environment:
#   REGISTRY_PULL_SECRET   Registry password/API key. If unset, setup does not
#                          create image pull secrets; images must be public,
#                          preloaded, or use existing imagePullSecrets.
#   REGISTRY_PULL_USERNAME Username for generated pull secrets.
#                          Default: $oauthtoken
#   NCX_REPO               Path to ncx-infra-controller-rest. Required only when
#                          REST is not skipped; preflight can auto-discover or
#                          clone it if missing.
#   NCX_SITE_UUID          Stable REST site UUID. Used only when REST is
#                          deployed. Default is a dev placeholder.
#   VAULT_NS               Vault namespace. Default: vault
#   CERT_MANAGER_NS        cert-manager namespace. Default: cert-manager
#   PREFLIGHT_CHECK_IMAGE  Image for preflight per-node checks.
#                          Default: busybox:1.36
#
# Usage:
#   export KUBECONFIG=/path/to/kubeconfig
#   export NCX_IMAGE_REGISTRY=<registry>    # unless using --skip-core --skip-rest
#   export NCX_CORE_IMAGE_TAG=<tag>       # unless using --skip-core
#   export NCX_REST_IMAGE_TAG=<tag>       # unless using --skip-rest
#   export REGISTRY_PULL_SECRET=<secret>  # optional
#   ./setup.sh                          # prompts before deploying Infra Controller Core and Infra Controller REST
#   ./setup.sh -y                       # skip all prompts, deploy everything automatically
#   ./setup.sh --skip-core              # skip Phase 6 Infra Controller Core (print command, deploy manually)
#   ./setup.sh --skip-rest              # skip Phase 7 Infra Controller REST entirely (no repo needed)
#   ./setup.sh --skip-core --skip-rest  # fully non-interactive infra-only run
#   ./setup.sh --core-values /path/to/values.yaml      # use site-specific values for Phase 6
#   ./setup.sh --metallb-config /path/to/metallb.yaml  # use site-specific MetalLB config (file or kustomize dir)
#   ./setup.sh --site-overlay /path/to/kustomize-dir   # kubectl apply -k after Phase 6 (NTP services, etc.)
#   ./setup.sh --debug                  # enable bash -x trace (or run: bash -x ./setup.sh)
#
# Notes:
#   - --core-values supplies site-specific Infra Controller Core Helm values.
#   - --metallb-config supplies site-specific MetalLB resources.
#   - --debug enables shell tracing and may print secrets; avoid it when
#     REGISTRY_PULL_SECRET is set unless logs are protected.
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${SCRIPT_DIR}"

AUTO_YES=false
SKIP_CORE=false
SKIP_REST=false
CORE_VALUES=""
METALLB_CONFIG=""
SITE_OVERLAY=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        -y)             AUTO_YES=true  ;;
        --skip-core)    SKIP_CORE=true ;;
        --skip-rest)    SKIP_REST=true ;;
        --debug)        set -x         ;;
        --core-values)
            [[ -z "${2:-}" ]] && { echo "Error: --core-values requires a file path"; exit 1; }
            CORE_VALUES="$(cd "$(dirname "$2")" && pwd)/$(basename "$2")"
            [[ ! -f "${CORE_VALUES}" ]] && { echo "Error: --core-values file not found: $2"; exit 1; }
            shift ;;
        --metallb-config)
            [[ -z "${2:-}" ]] && { echo "Error: --metallb-config requires a file or directory path"; exit 1; }
            METALLB_CONFIG="$(cd "$(dirname "$2")" && pwd)/$(basename "$2")"
            [[ ! -e "${METALLB_CONFIG}" ]] && { echo "Error: --metallb-config path not found: $2"; exit 1; }
            shift ;;
        --site-overlay)
            [[ -z "${2:-}" ]] && { echo "Error: --site-overlay requires a kustomize directory path"; exit 1; }
            SITE_OVERLAY="$(cd "$(dirname "$2")" && pwd)/$(basename "$2")"
            [[ ! -d "${SITE_OVERLAY}" ]] && { echo "Error: --site-overlay directory not found: $2"; exit 1; }
            shift ;;
        *) echo "Usage: $0 [-y] [--skip-core] [--skip-rest] [--core-values <file>] [--metallb-config <file-or-dir>] [--site-overlay <dir>] [--debug]"; exit 1 ;;
    esac
    shift
done

# ---------------------------------------------------------------------------
# Pre-flight checks — env vars, tools, config files, Infra Controller REST repo
# Exports NCX_REPO if resolved. Exits 1 if user declines to continue.
# ---------------------------------------------------------------------------
export AUTO_YES SKIP_CORE SKIP_REST
# shellcheck source=preflight.sh
source "${SCRIPT_DIR}/preflight.sh"

VAULT_NS="${VAULT_NS:-vault}"
CERT_MANAGER_NS="${CERT_MANAGER_NS:-cert-manager}"

# ---------------------------------------------------------------------------
# Failure handler — offer to run clean.sh if setup exits with an error.
# Registered AFTER preflight so preflight aborts don't trigger it.
# ---------------------------------------------------------------------------
_SETUP_PHASE="initializing"

_on_failure() {
    local _rc=$?
    local _cmd="${BASH_COMMAND}"
    [[ ${_rc} -eq 0 ]] && return              # clean exit — nothing to do
    [[ "${_SETUP_PHASE}" == "complete" ]] && return  # finished successfully

    echo ""
    echo "========================================================================="
    echo "  SETUP FAILED"
    echo "  Phase   : ${_SETUP_PHASE}"
    echo "  Command : ${_cmd}"
    echo "  Code    : ${_rc}"
    echo "========================================================================="
    echo ""
    echo "  The cluster may be in a partially installed state."
    echo "  clean.sh will remove all resources installed by this run and"
    echo "  return the cluster to a clean state."
    echo ""
    # Prompt only when this process can actually read from the controlling TTY.
    if ! { exec 3</dev/tty; } 2>/dev/null; then
        echo "  No interactive TTY — skipping cleanup prompt. To clean up manually:"
        echo "    ${SCRIPT_DIR}/clean.sh"
        return
    fi
    if ! read -r -p "  ➤  Run clean.sh to revert the cluster now? [y/N] " _clean_reply <&3; then
        exec 3<&-
        echo ""
        echo "  No interactive response — skipping cleanup prompt. To clean up manually:"
        echo "    ${SCRIPT_DIR}/clean.sh"
        return
    fi
    exec 3<&-
    echo ""
    if [[ "${_clean_reply:-N}" =~ ^[Yy]$ ]]; then
        echo "  Running clean.sh..."
        "${SCRIPT_DIR}/clean.sh" || true
        echo ""
        echo "  Cleanup complete. Fix the issue above and re-run setup.sh."
    else
        echo "  Skipped. To clean up manually:"
        echo "    ${SCRIPT_DIR}/clean.sh"
    fi
}
trap '_on_failure' EXIT

# ---------------------------------------------------------------------------
# Ensure helmfile is installed
# ---------------------------------------------------------------------------
if ! command -v helmfile &>/dev/null; then
    echo "helmfile not found — installing..."
    if command -v brew &>/dev/null; then
        brew install helmfile
    else
        # Download the latest release binary for Linux
        HELMFILE_VERSION="$(curl -fsSL https://api.github.com/repos/helmfile/helmfile/releases/latest \
            | grep '"tag_name"' | sed 's/.*"tag_name": *"v\([^"]*\)".*/\1/')"
        ARCH="$(uname -m)"
        [[ "${ARCH}" == "x86_64" ]] && ARCH="amd64"
        [[ "${ARCH}" == "aarch64" ]] && ARCH="arm64"
        curl -fsSL "https://github.com/helmfile/helmfile/releases/download/v${HELMFILE_VERSION}/helmfile_${HELMFILE_VERSION}_linux_${ARCH}.tar.gz" \
            | tar -xz -C /usr/local/bin helmfile
        chmod +x /usr/local/bin/helmfile
    fi
    echo "helmfile $(helmfile --version) installed"
fi

# ---------------------------------------------------------------------------
# DNS check — verify cluster DNS is working before proceeding.
#
# Two supported setups:
#   Kubespray clusters: NodeLocal DNSCache DaemonSet (nodelocaldns) in kube-system.
#                       The ConfigMap and ServiceAccount are created by Kubespray;
#                       this script deploys the DaemonSet if it is missing.
#   kubeadm / other:   CoreDNS Deployment in kube-system. NodeLocal DNSCache is
#                       not used — we just verify CoreDNS pods are ready.
#
# We detect which setup is present by checking for the Kubespray-created
# ConfigMap (nodelocaldns). If absent, we skip the nodelocaldns DaemonSet
# entirely and check CoreDNS instead.
# ---------------------------------------------------------------------------
_SETUP_PHASE="cluster DNS check"
echo "=== Checking cluster DNS ==="

if kubectl get configmap nodelocaldns -n kube-system &>/dev/null; then
    # Kubespray cluster — NodeLocal DNSCache is expected
    NODEDNS_READY="$(kubectl get daemonset nodelocaldns -n kube-system \
        -o jsonpath='{.status.numberReady}' 2>/dev/null || echo "0")"
    NODEDNS_DESIRED="$(kubectl get daemonset nodelocaldns -n kube-system \
        -o jsonpath='{.status.desiredNumberScheduled}' 2>/dev/null || echo "-1")"

    if [[ "${NODEDNS_READY}" == "${NODEDNS_DESIRED}" && \
          "${NODEDNS_DESIRED}" != "0" && "${NODEDNS_DESIRED}" != "-1" ]]; then
        echo "DNS OK — nodelocaldns ${NODEDNS_READY}/${NODEDNS_DESIRED} ready"
    else
        echo "NodeLocal DNSCache not ready (${NODEDNS_READY}/${NODEDNS_DESIRED}) — deploying DaemonSet..."
        # apply may fail with "selector immutable" if DaemonSet already exists
        kubectl apply -f operators/nodelocaldns-daemonset.yaml 2>/dev/null || true
        kubectl rollout status daemonset/nodelocaldns -n kube-system --timeout=120s
        echo "NodeLocal DNSCache ready — waiting 10s for iptables to converge..."
        sleep 10
    fi
else
    # kubeadm or other cluster — check CoreDNS instead
    COREDNS_READY="$(kubectl get deployment coredns -n kube-system \
        -o jsonpath='{.status.readyReplicas}' 2>/dev/null || echo "0")"
    COREDNS_DESIRED="$(kubectl get deployment coredns -n kube-system \
        -o jsonpath='{.spec.replicas}' 2>/dev/null || echo "0")"

    if [[ "${COREDNS_READY}" -ge 1 ]]; then
        echo "DNS OK — CoreDNS ${COREDNS_READY}/${COREDNS_DESIRED} ready (nodelocaldns not present, skipping)"
    else
        echo "WARNING: CoreDNS is not ready (${COREDNS_READY}/${COREDNS_DESIRED}) — DNS resolution may fail"
        echo "  Check CoreDNS pods: kubectl get pods -n kube-system -l k8s-app=kube-dns"
        echo "  Continuing — some later steps may fail if DNS is broken"
    fi
fi

# ---------------------------------------------------------------------------
# 1. local-path-provisioner (no Helm chart — raw manifest)
# ---------------------------------------------------------------------------
_SETUP_PHASE="[1/6] local-path-provisioner"
echo "=== [1/6] local-path-provisioner ==="
kubectl apply -f operators/local-path-provisioner.yaml
# StorageClass provisioner is immutable — delete before apply so a stale
# provisioner from a previous install doesn't block the update.
kubectl delete -f operators/storageclass-local-path-persistent.yaml \
    --ignore-not-found 2>/dev/null || true
kubectl apply -f operators/storageclass-local-path-persistent.yaml
kubectl rollout status deployment/local-path-provisioner -n local-path-storage --timeout=120s
# Mark local-path as the cluster default StorageClass so workloads that don't
# specify one (e.g. Infra Controller REST postgres, Temporal) get a valid provisioner.
kubectl annotate storageclass local-path \
    storageclass.kubernetes.io/is-default-class=true --overwrite

# ---------------------------------------------------------------------------
# 1b. postgres-operator — Zalando operator must be up (CRD registered) before
#     the Infra Controller prereqs chart creates the postgresql resource in Phase 5.
#     No TLS dependency — install early.
# ---------------------------------------------------------------------------
_SETUP_PHASE="[1b] postgres-operator"
echo "=== [1b] postgres-operator ==="
helmfile sync -l name=postgres-operator

# ---------------------------------------------------------------------------
# 1c. MetalLB — LoadBalancer service provider (BGP or L2 mode).
#     No TLS/PKI dependency — installed early so it is ready before Infra Controller Core
#     deploys LoadBalancer services (Infra Controller Core API, dhcp, dns, pxe, ssh-console-rs).
#
#     After the helm release installs the CRDs, site-specific config is applied
#     from --metallb-config <path> (file or kustomize dir) if provided, otherwise
#     from values/metallb-config.yaml. Fill in that file or pass --metallb-config.
# ---------------------------------------------------------------------------
_SETUP_PHASE="[1c] MetalLB"
echo "=== [1c] MetalLB ==="

helmfile sync -l name=metallb

echo "Waiting for MetalLB controller to be ready..."
kubectl wait --for=condition=Available deployment/metallb-controller \
    -n metallb-system --timeout=120s

echo "Applying MetalLB site config (IPAddressPool, BGPPeer, BGPAdvertisement)..."
if [[ -n "${METALLB_CONFIG}" ]]; then
    if [[ -d "${METALLB_CONFIG}" ]]; then
        kubectl apply -k "${METALLB_CONFIG}"
    else
        kubectl apply -f "${METALLB_CONFIG}"
    fi
else
    kubectl apply -f "${SCRIPT_DIR}/values/metallb-config.yaml"
fi
echo "MetalLB ready"

# ---------------------------------------------------------------------------
# 2. cert-manager + Prometheus CRDs + Vault TLS bootstrap
#    cert-manager must be up before we can issue certs for vault.
#    Vault pods need TLS secrets (forgeca-vault-client, vault-raft-tls)
#    BEFORE vault starts — so bootstrap them here via cert-manager.
# ---------------------------------------------------------------------------
_SETUP_PHASE="[2/6] cert-manager + Vault TLS bootstrap"
echo "=== [2/6] cert-manager + Vault TLS bootstrap ==="
helmfile sync -l name=cert-manager

kubectl apply --server-side -f operators/crds/ \
    --field-manager=helmfile --force-conflicts

kubectl create namespace "${VAULT_NS}" 2>/dev/null || true
helm template carbide-prereqs . \
    --namespace forge-system \
    --set imagePullSecrets.ngcCarbidePull="${REGISTRY_PULL_SECRET:-}" \
    --show-only templates/site-root-certificate.yaml \
    --show-only templates/vault-tls-certs.yaml \
    | kubectl apply --server-side --field-manager=helm -f -

kubectl wait --for=condition=Ready certificate/site-root \
    -n "${CERT_MANAGER_NS}" --timeout=120s
kubectl wait --for=condition=Ready certificate/forgeca-vault-client \
    -n "${VAULT_NS}" --timeout=120s
kubectl wait --for=condition=Ready certificate/vault-raft-tls \
    -n "${VAULT_NS}" --timeout=120s
echo "Vault TLS bootstrap complete"

# ---------------------------------------------------------------------------
# 3. vault — TLS secrets exist, pods can start
# ---------------------------------------------------------------------------
_SETUP_PHASE="[3/6] vault install"
echo "=== [3/6] vault ==="
helmfile sync -l name=vault

# ---------------------------------------------------------------------------
# 4. Initialize + unseal vault
#    Also sets up forge-system namespace (Helm labels + ssh-host-key)
#    so the Infra Controller prereqs helm install can adopt it.
# ---------------------------------------------------------------------------
_SETUP_PHASE="[4/6] vault init + unseal"
echo "=== [4/6] unseal vault ==="
./unseal_vault.sh
./bootstrap_ssh_host_key.sh

# ---------------------------------------------------------------------------
# 5. external-secrets + Infra Controller prereqs
# ---------------------------------------------------------------------------
_SETUP_PHASE="[5/6] external-secrets + Infra Controller prereqs"
echo "=== [5/6] external-secrets + Infra Controller prereqs ==="
helmfile sync -l name=external-secrets
helmfile sync -l name=carbide-prereqs

# ---------------------------------------------------------------------------
# Wait for postgres-operator to provision the cluster and ESO to sync creds
# before Infra Controller Core starts (the Infra Controller Core API needs the DB credentials Secret).
# ---------------------------------------------------------------------------
echo "Waiting for forge-pg-cluster to reach Running state..."
until kubectl get postgresql forge-pg-cluster -n postgres \
    -o jsonpath='{.status.PostgresClusterStatus}' 2>/dev/null | grep -q "Running"; do
    STATUS="$(kubectl get postgresql forge-pg-cluster -n postgres \
        -o jsonpath='{.status.PostgresClusterStatus}' 2>/dev/null || echo 'unknown')"
    echo "  forge-pg-cluster status: ${STATUS} — retrying in 10s..."
    sleep 10
done
echo "forge-pg-cluster is Running"

echo "Waiting for DB credentials to be synced by ESO..."
until kubectl get secret forge-system.carbide.forge-pg-cluster.credentials \
    -n forge-system &>/dev/null; do
    echo "  credentials not yet synced — retrying in 5s..."
    sleep 5
done
echo "DB credentials ready"

echo "Waiting for Vault AppRole credentials to be synced by ESO..."
until ROLE_ID_B64="$(kubectl get secret carbide-vault-approle-tokens \
        -n forge-system -o jsonpath='{.data.VAULT_ROLE_ID}' 2>/dev/null)" && \
      SECRET_ID_B64="$(kubectl get secret carbide-vault-approle-tokens \
        -n forge-system -o jsonpath='{.data.VAULT_SECRET_ID}' 2>/dev/null)" && \
      [[ -n "${ROLE_ID_B64}" && -n "${SECRET_ID_B64}" ]]; do
    echo "  AppRole credentials not yet synced — retrying in 5s..."
    sleep 5
done
echo "Vault AppRole credentials ready"

if ! "${SKIP_CORE}"; then
    # Create imagepullsecret in forge-system so the API migrate hook can pull its
    # image. The hook runs before chart resources are created, so this must exist
    # before helm install — not as a post-install manual step.
    # Skipped when REGISTRY_PULL_SECRET is unset (air-gapped / pre-loaded registry).
    if [[ -n "${REGISTRY_PULL_SECRET:-}" ]]; then
        _registry_server="${NCX_IMAGE_REGISTRY%%/*}"
        echo "Creating imagepullsecret in forge-system (server: ${_registry_server})..."
        kubectl create secret docker-registry imagepullsecret \
            --namespace forge-system \
            --docker-server="${_registry_server}" \
            --docker-username="${REGISTRY_PULL_USERNAME:-\$oauthtoken}" \
            --docker-password="${REGISTRY_PULL_SECRET}" \
            --dry-run=client -o yaml | kubectl apply -f -
    else
        echo "REGISTRY_PULL_SECRET not set — skipping imagepullsecret creation (air-gapped or pre-loaded registry)."
    fi
fi

# ---------------------------------------------------------------------------
# Infra Controller Core
# ---------------------------------------------------------------------------
if "${SKIP_CORE}"; then
    echo "=== [6/6] Infra Controller Core ==="
    echo "Skipped (--skip-core flag set)."
else
    _CORE_VALUES_FILE="${CORE_VALUES:-${SCRIPT_DIR}/values/ncx-core.yaml}"
    _CORE_VALUES_ARG="${CORE_VALUES:-helm-prereqs/values/ncx-core.yaml}"

    NCX_CORE_CMD=(
        helm upgrade --install carbide ./helm
        --namespace forge-system
        -f "${_CORE_VALUES_ARG}"
        --set-string "global.image.repository=${NCX_IMAGE_REGISTRY}/nvmetal-carbide"
        --set-string "global.image.tag=${NCX_CORE_IMAGE_TAG}"
        --timeout 300s --wait
    )
    _NCX_CORE_CMD_DISPLAY=""
    for _arg in "${NCX_CORE_CMD[@]}"; do
        printf -v _quoted_arg '%q' "${_arg}"
        _NCX_CORE_CMD_DISPLAY="${_NCX_CORE_CMD_DISPLAY}${_NCX_CORE_CMD_DISPLAY:+ }${_quoted_arg}"
    done

    # Warn if ncx-core.yaml still contains example placeholder values.
    if [[ -z "${CORE_VALUES}" ]] && \
       grep -q "api-examplesite.example.com\|sitename = \"examplesite\"\|examplesite.example.com" \
            "${SCRIPT_DIR}/values/ncx-core.yaml" 2>/dev/null; then
        echo "WARNING: values/ncx-core.yaml still contains example placeholder values."
        echo "  Update carbide-api.hostname, sitename, initial_domain_name, dhcp_servers,"
        echo "  site_fabric_prefixes, deny_prefixes, pools, and networks for your site."
        echo "  Or use --core-values /path/to/your-site-values.yaml to skip ncx-core.yaml."
        echo ""
    fi

    echo ""
    echo "========================================================================="
    echo "  ACTION REQUIRED: Before deploying Infra Controller Core, confirm you have updated:"
    echo "    ${_CORE_VALUES_FILE}"
    echo ""
    echo "  Key fields:"
    echo "    global.image.repository   — ${NCX_IMAGE_REGISTRY}/nvmetal-carbide"
    echo "    global.image.tag          — ${NCX_CORE_IMAGE_TAG}"
    echo "    carbide-api.hostname      — your site hostname"
    echo "    carbide-api.siteConfig    — site-specific network/pool/IB config"
    echo "========================================================================="
    echo ""
    if "${AUTO_YES}"; then
        _reply="Y"
    else
        read -r -p "  ➤  Deploy Infra Controller Core now? [Y/n] " _reply
        echo ""
    fi
    if [[ "${_reply:-Y}" =~ ^[Yy]$ ]]; then
        _SETUP_PHASE="[6/6] Infra Controller Core"
        echo "=== [6/6] Infra Controller Core ==="
        (cd "${SCRIPT_DIR}/.." && "${NCX_CORE_CMD[@]}")
    else
        echo "Skipped. To deploy manually, run from $(dirname "${SCRIPT_DIR}"):"
        echo "  ${_NCX_CORE_CMD_DISPLAY}"
    fi
fi

# ---------------------------------------------------------------------------
# Site kustomize overlay — applies site-specific resources that are not
# managed by the NVIDIA Infra Controller Helm chart (e.g. per-pod LoadBalancer Services,
# additional StatefulSets, or supplemental MetalLB config). Idempotent.
# ---------------------------------------------------------------------------
if [[ -n "${SITE_OVERLAY}" ]]; then
    _SETUP_PHASE="site overlay"
    echo "=== Site overlay: $(basename "${SITE_OVERLAY}") ==="
    kubectl apply -k "${SITE_OVERLAY}"
    echo "Site overlay applied"
fi

# ---------------------------------------------------------------------------
# 7. Infra Controller REST full stack
#    Order of operations:
#      7a. Resolve Infra Controller REST repo + CA signing secret
#      7b. Infra Controller REST CA issuer ClusterIssuer (cert-manager.io)
#      7c. Infra Controller REST postgres (simple StatefulSet — temporal + forge DBs)
#      7d. Keycloak (dev IdP)
#      7e. Temporal namespace + TLS certs (issued by the Infra Controller REST CA issuer)
#      7f. Temporal helm chart
#      7g. Infra Controller REST helm chart (API, cert-manager, workflow, site-manager)
# ---------------------------------------------------------------------------
echo ""
_SETUP_PHASE="[7/7] Infra Controller REST"
echo "=== [7/7] Infra Controller REST ==="

if "${SKIP_REST}"; then
    echo "Skipped (--skip-rest flag set)."
    echo ""
    echo "=== Setup complete (Infra Controller REST skipped) ==="
    _SETUP_PHASE="complete"
    exit 0
fi

# --- 7a. Infra Controller REST repo (resolved and exported by preflight.sh) -------------------
if [[ -z "${NCX_REPO:-}" ]]; then
    echo "ERROR: Infra Controller REST repo is not set. Re-run setup.sh and choose to clone, or:"
    echo "  export NCX_REPO=/path/to/<ncx-rest-repo>   # e.g. ncx or ncx-infra-controller-rest"
    exit 1
fi
echo "Infra Controller REST repo: ${NCX_REPO}"

# Create Infra Controller REST namespace
kubectl create namespace carbide-rest 2>/dev/null || true

# CA signing secret — needed by the Infra Controller REST cert-manager component (internal PKI)
# and the cert-manager.io ClusterIssuer. gen-site-ca.sh creates it in
# both the Infra Controller REST and cert-manager namespaces in one shot.
if kubectl get secret ca-signing-secret -n carbide-rest &>/dev/null; then
    echo "ca-signing-secret already present — skipping CA generation"
else
    echo "Generating Infra Controller REST CA signing secret..."
    (cd "${NCX_REPO}" && ./scripts/gen-site-ca.sh)
fi

# --- 7b. ClusterIssuer -------------------------------------------------------
_SETUP_PHASE="[7b/7] Infra Controller REST CA issuer ClusterIssuer"
echo "=== [7b/7] Infra Controller REST CA issuer ClusterIssuer ==="
(cd "${NCX_REPO}" && kubectl apply -k deploy/kustomize/base/cert-manager-io)

# --- 7c. Infra Controller REST postgres --------------------------------------------------------
# Simple postgres StatefulSet with all Infra Controller databases pre-initialised:
# forge, temporal, temporal_visibility, keycloak.
# Lives alongside forge-pg-cluster in the postgres namespace — different
# service name ("postgres") so Temporal and Infra Controller values work without changes.
_SETUP_PHASE="[7c/7] Infra Controller REST postgres"
echo "=== [7c/7] Infra Controller REST postgres ==="
(cd "${NCX_REPO}" && kubectl apply -k deploy/kustomize/base/postgres)
kubectl rollout status statefulset/postgres -n postgres --timeout=180s
echo "Infra Controller REST postgres ready"

# --- 7d. Keycloak (conditional) -----------------------------------------------
# Only deploy Keycloak if ncx-rest.yaml has keycloak.enabled: true.
# If using external OAuth2/OIDC (Option B in ncx-rest.yaml), skip this step.
# Dev OIDC IdP, pre-loaded with the configured Infra Controller development realm + test users.
# carbide-rest-api talks to it at http://keycloak.carbide-rest:8082
_SETUP_PHASE="[7d/7] Keycloak"
_KC_ENABLED="$(grep -A5 'keycloak:' "${SCRIPT_DIR}/values/ncx-rest.yaml" \
    | grep 'enabled:' | head -1 | awk '{print $2}' || echo "false")"

if [[ "${_KC_ENABLED}" == "true" ]]; then
    echo "=== [7d/7] Keycloak ==="
    "${SCRIPT_DIR}/keycloak/setup.sh"
    echo "Keycloak ready"
else
    echo "=== [7d/7] Keycloak — skipped (keycloak.enabled is not true in ncx-rest.yaml) ==="
fi

# --- 7e. Temporal namespace + TLS certs + db-creds --------------------------
_SETUP_PHASE="[7e/7] Temporal TLS bootstrap"
echo "=== [7e/7] Temporal TLS bootstrap ==="
(cd "${NCX_REPO}" && kubectl apply -f deploy/kustomize/base/temporal-helm/namespace.yaml)
(cd "${NCX_REPO}" && kubectl apply -f deploy/kustomize/base/temporal-helm/db-creds.yaml)
(cd "${NCX_REPO}" && kubectl apply -f deploy/kustomize/base/temporal-helm/certificates.yaml)

echo "Waiting for temporal TLS certificates to be issued..."
kubectl wait --for=condition=Ready certificate/server-interservice-cert \
    -n temporal --timeout=120s
kubectl wait --for=condition=Ready certificate/server-cloud-cert \
    -n temporal --timeout=120s
kubectl wait --for=condition=Ready certificate/server-site-cert \
    -n temporal --timeout=120s
echo "Temporal TLS certs ready"

# --- 7f. Temporal ------------------------------------------------------------
_SETUP_PHASE="[7f/7] Temporal"
echo "=== [7f/7] Temporal ==="
helm upgrade --install temporal "${NCX_REPO}/temporal-helm/temporal" \
    --namespace temporal \
    -f "${NCX_REPO}/temporal-helm/temporal/values-kind.yaml" \
    --timeout 300s --wait
echo "Temporal ready"

# Create the Temporal namespaces required by Infra Controller REST workers (requires mTLS)
echo "Creating Temporal cloud and site namespaces..."
_TEMPORAL_ADDR="temporal-frontend.temporal:7233"
_TEMPORAL_TLS="--tls-cert-path /var/secrets/temporal/certs/server-interservice/tls.crt \
    --tls-key-path /var/secrets/temporal/certs/server-interservice/tls.key \
    --tls-ca-path /var/secrets/temporal/certs/server-interservice/ca.crt \
    --tls-server-name interservice.server.temporal.local"
kubectl exec -n temporal deploy/temporal-admintools -- \
    sh -c "temporal operator namespace create -n cloud --address ${_TEMPORAL_ADDR} ${_TEMPORAL_TLS}" 2>/dev/null || true
kubectl exec -n temporal deploy/temporal-admintools -- \
    sh -c "temporal operator namespace create -n site --address ${_TEMPORAL_ADDR} ${_TEMPORAL_TLS}" 2>/dev/null || true
echo "Temporal namespaces ready"

_SETUP_PHASE="[7g/7] Infra Controller REST helm chart"
# --- 7g. Infra Controller REST helm chart -------------------------------------------------
NCX_HELM_CHART="${NCX_REPO}/helm/charts/carbide-rest"
NCX_REST_CMD=(
    helm upgrade --install carbide-rest "${NCX_HELM_CHART}"
    --namespace carbide-rest
    -f "${SCRIPT_DIR}/values/ncx-rest.yaml"
    --set global.image.repository="${NCX_IMAGE_REGISTRY}"
    --set global.image.tag="${NCX_REST_IMAGE_TAG}"
    --timeout 600s --wait
)

if [[ -n "${REGISTRY_PULL_SECRET:-}" ]]; then
    # Build dockerconfigjson for the image-pull-secret that the Infra Controller REST common
    # chart creates. The registry host is derived from NCX_IMAGE_REGISTRY so this
    # works for nvcr.io and private non-NGC registries.
    _ncx_registry_server="${NCX_IMAGE_REGISTRY%%/*}"
    _ncx_docker_cfg="$(printf '{"auths":{"%s":{"username":"%s","password":"%s"}}}' \
        "${_ncx_registry_server}" \
        "${REGISTRY_PULL_USERNAME:-\$oauthtoken}" \
        "${REGISTRY_PULL_SECRET}" | base64 | tr -d '\n')"
    NCX_REST_CMD+=(
        --set "carbide-rest-common.secrets.imagePullSecret.dockerconfigjson=${_ncx_docker_cfg}"
    )
else
    echo "REGISTRY_PULL_SECRET not set — omitting Infra Controller REST image pull secret override."
    echo "Infra Controller REST images must be public, preloaded, or configured with existing imagePullSecrets in values."
fi

echo ""
echo "========================================================================="
echo "  Infra Controller REST"
echo "    Image:  ${NCX_IMAGE_REGISTRY}  tag: ${NCX_REST_IMAGE_TAG}"
echo "    Values: ${SCRIPT_DIR}/values/ncx-rest.yaml"
echo "    Auth:   Keycloak dev instance (step 7d) — update ncx-rest.yaml for production IdP"
echo "========================================================================="
echo ""
if "${AUTO_YES}"; then
    _ncx_reply="Y"
else
    read -r -p "  ➤  Deploy Infra Controller REST now? [Y/n] " _ncx_reply
    echo ""
fi
if [[ "${_ncx_reply:-Y}" =~ ^[Yy]$ ]]; then
    "${NCX_REST_CMD[@]}"
else
    echo "Skipped Infra Controller REST. Re-run with -y or answer Y to deploy."
    echo ""
    echo "=== Setup complete (Infra Controller REST skipped) ==="
    exit 0
fi

# --- 7h. Infra Controller REST site-agent -------------------------------------------------
# The site-agent is a separate chart from the main Infra Controller REST umbrella.
#
# Bootstrap order:
#   1. Create the per-site Temporal namespace BEFORE helm install so the
#      site-agent never starts without it (starting without it causes an
#      immediate nil-pointer panic in RegisterCron).
#   2. Install the chart with bootstrap.enabled=true — a pre-install Helm hook
#      Job (alpine/k8s) runs entirely inside the cluster:
#        a. Calls POST carbide-rest-site-manager:8100/v1/site to register the site.
#        b. Waits for the Site CR OTP (populated by site-manager operator).
#        c. Creates site-registration secret with real UUID + OTP.
#      The StatefulSet pod is only created AFTER the hook completes, so there is
#      no FailedMount window. Do NOT pre-create the secret — that would trigger
#      the Job's idempotency check and skip the real bootstrap.
#
# The site-agent binary also needs DB credentials for its local elektratest DB.
# All of this is wired via --set flags so ncx-rest.yaml stays registry-agnostic.
NCX_SITE_AGENT_CHART="${NCX_REPO}/helm/charts/carbide-rest-site-agent"

# Stable placeholder UUID for this site (must be a valid UUID).
NCX_SITE_UUID="${NCX_SITE_UUID:-a1b2c3d4-e5f6-4000-8000-000000000001}"

NCX_SITE_AGENT_ARGS=(
    --namespace carbide-rest
    -f "${SCRIPT_DIR}/values/ncx-site-agent.yaml"
    --set global.image.repository="${NCX_IMAGE_REGISTRY}"
    --set global.image.tag="${NCX_REST_IMAGE_TAG}"
)
if [[ -n "${REGISTRY_PULL_SECRET:-}" ]]; then
    NCX_SITE_AGENT_ARGS+=(
        --set "global.imagePullSecrets[0].name=image-pull-secret"
    )
fi

_SETUP_PHASE="[7h/7] Infra Controller REST site-agent"
echo "=== [7h/7] Infra Controller REST site-agent (site UUID: ${NCX_SITE_UUID}) ==="

# Pre-apply the Certificate resource so cert-manager issues the Infra Controller gRPC client
# cert BEFORE the StatefulSet pod starts. Without this, there is a race: helm creates
# both the Certificate and the StatefulSet simultaneously, and the pod's
# GetInitialCertMD5() call fails because the secret hasn't been projected yet.
echo "Pre-applying Infra Controller gRPC client certificate..."
# Issue the cert from vault-forge-issuer (same CA as the Infra Controller Core API) so that:
#   1. the Infra Controller Core API trusts the site-agent's client cert (Vault PKI CA)
#   2. the ca.crt in the secret is the Vault PKI CA, which the site-agent uses
#      as ServerCAPath to verify the Infra Controller Core API server cert (also Vault-signed)
# Use the same values file as the install step so the rendered Certificate is
# byte-for-byte identical — preventing cert-manager from re-issuing the cert.
helm template carbide-rest-site-agent "${NCX_SITE_AGENT_CHART}" \
    "${NCX_SITE_AGENT_ARGS[@]}" \
    --show-only templates/certificate.yaml | kubectl apply -f -
# Add Helm ownership annotations so the subsequent helm install can adopt this resource
# instead of failing with "exists and cannot be imported into the current release".
kubectl annotate certificate/core-grpc-client-site-agent-certs -n carbide-rest \
    "meta.helm.sh/release-name=carbide-rest-site-agent" \
    "meta.helm.sh/release-namespace=carbide-rest" --overwrite
kubectl label certificate/core-grpc-client-site-agent-certs -n carbide-rest \
    "app.kubernetes.io/managed-by=Helm" --overwrite
echo "Waiting for cert-manager to issue core-grpc-client-site-agent-certs..."
kubectl wait --for=condition=Ready certificate/core-grpc-client-site-agent-certs \
    -n carbide-rest --timeout=120s
echo "Infra Controller gRPC client cert ready"

# Create per-site Temporal namespace BEFORE deploying site-agent.
# The site-agent panics immediately on startup if this namespace doesn't exist.
echo "Creating Temporal namespace for site ${NCX_SITE_UUID}..."
_TEMPORAL_ADDR="temporal-frontend.temporal:7233"
_TEMPORAL_TLS="--tls-cert-path /var/secrets/temporal/certs/server-interservice/tls.crt \
    --tls-key-path /var/secrets/temporal/certs/server-interservice/tls.key \
    --tls-ca-path /var/secrets/temporal/certs/server-interservice/ca.crt \
    --tls-server-name interservice.server.temporal.local"
kubectl exec -n temporal deploy/temporal-admintools -- \
    sh -c "temporal operator namespace create -n '${NCX_SITE_UUID}' --address ${_TEMPORAL_ADDR} ${_TEMPORAL_TLS}" 2>/dev/null || true
echo "Temporal namespace ready"

helm upgrade --install carbide-rest-site-agent "${NCX_SITE_AGENT_CHART}" \
    "${NCX_SITE_AGENT_ARGS[@]}" \
    --set "envConfig.CLUSTER_ID=${NCX_SITE_UUID}" \
    --set "envConfig.TEMPORAL_SUBSCRIBE_NAMESPACE=${NCX_SITE_UUID}" \
    --set "envConfig.TEMPORAL_SUBSCRIBE_QUEUE=site" \
    --timeout 300s --wait
echo "Infra Controller REST site-agent deployed and bootstrap complete"

# Verify the site-agent's gRPC connection to Infra Controller Core succeeded. The site-agent attempts
# the connection exactly once at startup with a 5-second deadline; if it
# fails for any transient reason the CarbideClient stays nil permanently and
# all inventory activities panic.  Detect failure and restart the pod so it
# gets a fresh attempt with the same correct config.
echo "Verifying site-agent Infra Controller Core gRPC connection..."
_CONNECTED=false
for _i in $(seq 1 24); do
    _POD="$(kubectl get pods -n carbide-rest \
        -l "app.kubernetes.io/name=carbide-rest-site-agent" \
        -o name 2>/dev/null | head -1)"
    if [ -n "${_POD}" ] && \
       kubectl logs -n carbide-rest "${_POD}" --since=5m 2>/dev/null \
           | grep -q "CarbideClient: successfully connected to server"; then
        _CONNECTED=true
        echo "Site-agent successfully connected to Infra Controller Core gRPC"
        break
    fi
    echo "  Waiting for gRPC connection (${_i}/24)..."
    sleep 5
done

if [ "${_CONNECTED}" = "false" ]; then
    echo "WARNING: site-agent did not confirm gRPC connection — restarting pod for retry..."
    kubectl rollout restart statefulset/carbide-rest-site-agent -n carbide-rest
    kubectl rollout status statefulset/carbide-rest-site-agent -n carbide-rest --timeout=120s
    echo "Site-agent pod restarted — gRPC connection will be retried"
fi

echo ""
echo "========================================================================="
echo "  Setup complete"
echo "========================================================================="
echo ""
echo "  Quick health checks:"
echo "    kubectl get clusterissuer"
echo "    kubectl get secret forge-roots -n forge-system"
echo "    kubectl get pods -n forge-system"
echo "    kubectl get pods -n carbide-rest"
echo "    kubectl get pods -n temporal"
echo ""
echo "  Next steps — see helm-prereqs/README.md, section 8:"
if [[ "${_KC_ENABLED:-false}" == "true" ]]; then
    echo "    • Acquiring a Keycloak access token     (helper: ${SCRIPT_DIR}/keycloak/get-token.sh)"
else
    echo "    • Acquiring an access token             (Keycloak disabled — use your own IdP)"
fi
echo "    • Setting up the Infra Controller CLI against this cluster"
echo "    • Bootstrap the org and create your first site"
echo "    • Next: IP blocks and downstream resources"
echo ""
echo "  Keycloak deep-dive (realm, clients, roles): helm-prereqs/keycloak/README.md"
echo "========================================================================="

_SETUP_PHASE="complete"  # signals _on_failure trap: clean exit, no prompt needed
