# Carbide Helm Chart

NVIDIA Bare Metal Manager (Carbide) -- Kubernetes Deployment

## Overview

Carbide (also known as NVIDIA Bare Metal Manager) is a platform for provisioning, managing, and monitoring bare metal GPU servers, including DGX and HGX systems. This Helm chart deploys all Carbide services into a Kubernetes cluster as a single umbrella chart with 13 independently toggleable subcharts.

The chart is designed for production environments where Carbide manages the full lifecycle of bare metal infrastructure: DHCP/PXE-based OS provisioning, DNS resolution, BGP network peering, hardware health monitoring, SSH console access, NTP time synchronization, and a unified REST/gRPC API.

## Subcharts

| # | Subchart | Description |
|---|----------|-------------|
| 1 | **carbide-api** | Core API server (gRPC + REST). Manages machines, provisioning, networking, and firmware. Requires PostgreSQL and Vault. |
| 2 | **carbide-dhcp** | DHCP server (Kea-based) for bare metal PXE boot and network assignment. |
| 3 | **carbide-dns** | Authoritative DNS server for managed machines and VPCs. |
| 4 | **carbide-dsx-exchange-consumer** | Consumes DSX exchange messages for machine telemetry and state updates. |
| 5 | **carbide-hardware-health** | Collects and reports hardware health metrics from managed machines. |
| 6 | **carbide-nginx** | Nginx reverse proxy for serving boot artifacts and the web UI. |
| 7 | **carbide-ntp** | Chrony NTP server providing time synchronization to bare metal hosts. |
| 8 | **carbide-nvpasswd-unexpirer** | DaemonSet that prevents password expiry on managed nodes. |
| 9 | **carbide-pxe** | PXE boot server (HTTP-based) for OS provisioning workflows. |
| 10 | **carbide-ssh-console-rs** | SSH console proxy for remote access to managed machine BMCs and consoles. |
| 11 | **carbide-ufm** | UFM (Unified Fabric Manager) scrape config for InfiniBand monitoring. |
| 12 | **frrouting** | FRRouting BGP daemon for network peering and route advertisement. |
| 13 | **unbound** | Recursive DNS resolver forwarding queries for managed infrastructure. |

## Prerequisites

- **Kubernetes** 1.27+
- **Helm** 3.12+
- **cert-manager** with a `ClusterIssuer` configured (default issuer name: `vault-forge-issuer`)
- **HashiCorp Vault** for PKI certificate issuance and secret storage
- **PostgreSQL** (SSL-enabled) for the `carbide-api` database backend
- **Prometheus Operator CRDs** if you enable `ServiceMonitor` or `PodMonitor` resources
- **Required Kubernetes Secrets and ConfigMaps** (Vault tokens, database credentials, SSO secrets, etc.)

For the full list of required secrets, ConfigMaps, and infrastructure setup steps, see [PREREQUISITES.md](./PREREQUISITES.md).

## Quick Start

```bash
helm upgrade --install carbide ./helm \
  --namespace forge-system --create-namespace \
  --set global.image.repository=<your-registry>/carbide-core \
  --set global.image.tag=<version> \
  --set global.namespace=forge-system
```

To verify the deployment:

```bash
kubectl get pods -n forge-system
kubectl get svc -n forge-system
```

## Configuration

### Global Values

Top-level `global:` values are automatically passed to all subcharts.

| Parameter | Description | Default |
|-----------|-------------|---------|
| `global.namespace` | Target namespace for all resources | `default` |
| `global.image.repository` | Container image repository (**REQUIRED**) | `""` |
| `global.image.tag` | Container image tag (**REQUIRED**) | `""` |
| `global.image.pullPolicy` | Image pull policy | `IfNotPresent` |
| `global.imagePullSecrets` | Image pull secrets | `[]` |
| `global.certificate.duration` | Certificate validity period | `720h0m0s` |
| `global.certificate.renewBefore` | Renew certificates before expiry | `360h0m0s` |
| `global.certificate.privateKey.algorithm` | Certificate private key algorithm | `ECDSA` |
| `global.certificate.privateKey.size` | Certificate private key size | `384` |
| `global.certificate.issuerRef.name` | cert-manager ClusterIssuer name | `vault-forge-issuer` |
| `global.certificate.issuerRef.kind` | cert-manager issuer kind | `ClusterIssuer` |
| `global.certificate.issuerRef.group` | cert-manager issuer API group | `cert-manager.io` |
| `global.spiffe.trustDomain` | SPIFFE trust domain for mTLS | `forge.local` |
| `global.labels` | Common labels applied to all resources | See `values.yaml` |

### Subchart Enable/Disable Flags

Each subchart can be independently enabled or disabled. All subcharts are enabled by default.

```yaml
carbide-api:
  enabled: true        # Core API -- usually always enabled
carbide-dhcp:
  enabled: true        # DHCP for PXE boot
carbide-dns:
  enabled: true        # Authoritative DNS
carbide-dsx-exchange-consumer:
  enabled: true        # DSX exchange telemetry consumer
carbide-hardware-health:
  enabled: true        # Hardware health monitoring
carbide-nginx:
  enabled: true        # Nginx reverse proxy / web UI
carbide-ntp:
  enabled: true        # NTP time server
carbide-nvpasswd-unexpirer:
  enabled: true        # Password unexpiry DaemonSet
carbide-pxe:
  enabled: true        # PXE boot server
carbide-ssh-console-rs:
  enabled: true        # SSH console proxy
carbide-ufm:
  enabled: true        # UFM InfiniBand scrape config
frrouting:
  enabled: true        # BGP routing daemon
unbound:
  enabled: true        # Recursive DNS resolver
```

### Image Configuration

The `global.image.repository` and `global.image.tag` values **must** be set -- they default to empty strings. Most subcharts use the global image reference. The following subcharts use their own separate image references and do **not** inherit `global.image`:

| Subchart | Image Parameter | Default Repository |
|----------|----------------|--------------------|
| `carbide-ntp` | `carbide-ntp.image.repository` / `.tag` | `dockurr/chrony` |
| `carbide-nvpasswd-unexpirer` | `carbide-nvpasswd-unexpirer.image.repository` / `.tag` | `ubuntu:jammy` |
| `frrouting` | `frrouting.image.repository` / `.tag` | `""` (must be set) |
| `frrouting` (exporter) | `frrouting.exporterImage.repository` / `.tag` | `ghcr.io/tynany/frr_exporter` |
| `unbound` | `unbound.image.repository` / `.tag` | `""` (must be set) |
| `unbound` (exporter) | `unbound.exporterImage.repository` / `.tag` | `""` (must be set) |

### OAuth2 / SSO Setup

To enable OAuth2 authentication (for example, Azure AD or Okta), configure the `carbide-api` environment variables:

```yaml
carbide-api:
  env:
    CARBIDE_WEB_AUTH_TYPE: "oauth2"
    CARBIDE_WEB_OAUTH2_AUTH_ENDPOINT: "https://your-idp/authorize"
    CARBIDE_WEB_OAUTH2_TOKEN_ENDPOINT: "https://your-idp/token"
    CARBIDE_WEB_OAUTH2_CLIENT_ID: "your-client-id"
    CARBIDE_WEB_ALLOWED_ACCESS_GROUPS: "group1,group2"
```

The OAuth2 client secret should be provided via a Kubernetes Secret referenced in `carbide-api.envFrom.azureSsoSecret.secretName`.

### External LoadBalancer Services

Several services support optional external LoadBalancer exposure, typically used with MetalLB on bare metal clusters. Enable and configure them per subchart:

```yaml
carbide-api:
  externalService:
    enabled: true
    type: LoadBalancer
    externalTrafficPolicy: Local
    annotations:
      metallb.universe.tf/loadBalancerIPs: "10.x.x.x"
```

Services with external LoadBalancer support: `carbide-api`, `carbide-dhcp`, `carbide-dns`, `carbide-nginx`, `carbide-ntp`, `carbide-pxe`, `carbide-ssh-console-rs`, and `frrouting`.

For StatefulSet-based services (`carbide-dns`, `carbide-ntp`, `frrouting`), per-pod LoadBalancer IPs can be assigned:

```yaml
carbide-dns:
  externalService:
    enabled: true
    perPodAnnotations:
      - metallb.universe.tf/loadBalancerIPs: "10.x.x.1"   # pod-0
      - metallb.universe.tf/loadBalancerIPs: "10.x.x.2"   # pod-1
```

## Architecture

### Workload Summary

| Subchart | Workload Type | Primary Port(s) | TLS Certificate | Metrics |
|----------|--------------|-----------------|-----------------|---------|
| carbide-api | Deployment | 1079 (gRPC), 1080 (metrics), 1081 (profiler) | Yes | ServiceMonitor |
| carbide-dhcp | Deployment | 67/UDP, 1089 (metrics) | Yes | ServiceMonitor |
| carbide-dns | StatefulSet | 53/TCP, 53/UDP | Yes | -- |
| carbide-dsx-exchange-consumer | Deployment | 9009 | Yes | ServiceMonitor |
| carbide-hardware-health | Deployment | 9009 | Yes | ServiceMonitor |
| carbide-nginx | Deployment | 80 | Yes | -- |
| carbide-ntp | StatefulSet | 123/UDP | No | -- |
| carbide-nvpasswd-unexpirer | DaemonSet | -- | No | -- |
| carbide-pxe | Deployment | 8080 | Yes | ServiceMonitor |
| carbide-ssh-console-rs | Deployment | 22, 9009 (metrics) | Yes | ServiceMonitor |
| carbide-ufm | -- (ScrapeConfig only) | -- | No | ScrapeConfig |
| frrouting | StatefulSet | 179 (BGP) | No | PodMonitor |
| unbound | Deployment | 53 | No | ServiceMonitor |

### Service Dependencies

```
                         +------------------+
                         |   carbide-api    |  <-- PostgreSQL, Vault
                         +--------+---------+
                                  |
          +-----------+-----------+-----------+-----------+
          |           |           |           |           |
    carbide-dhcp  carbide-dns  carbide-pxe  carbide-nginx  carbide-ssh-console-rs
          |                       |
          v                       v
     Bare Metal            Bare Metal
     (PXE boot)            (OS install)

    frrouting (BGP) <---> Network Fabric
    carbide-ntp     <---> Bare Metal (time sync)
    unbound         <---> Upstream DNS
```

All services that communicate with `carbide-api` use mTLS via SPIFFE-based certificates issued by cert-manager and backed by Vault PKI.

## Examples

For reference configurations, see:

- [`examples/values-minimal.yaml`](./examples/values-minimal.yaml) -- Minimal deployment with only the core services
- [`examples/values-full.yaml`](./examples/values-full.yaml) -- Full deployment with all services and production settings

## Migrating from Kustomize

This Helm chart supersedes the Kustomize-based deployment previously located in `deploy/`. The mapping is straightforward:

- Each Kustomize component maps to a subchart with the same name.
- Base resources (Deployments, Services, ConfigMaps) are now templated within each subchart.
- Environment-specific configuration that was previously managed through Kustomize overlays should be provided via Helm values overrides (`-f values-myenv.yaml` or `--set` flags).
- ConfigMap generators in Kustomize are replaced by `config:` sections in each subchart's values, with the option to provide external ConfigMaps instead (`config.enabled: false`).

## Upgrading

```bash
helm upgrade carbide ./helm \
  --namespace forge-system \
  -f values-production.yaml
```

Review changes before applying:

```bash
helm diff upgrade carbide ./helm \
  --namespace forge-system \
  -f values-production.yaml
```

## Uninstalling

```bash
helm uninstall carbide --namespace forge-system
```

Note that PersistentVolumeClaims, Secrets, and ConfigMaps created outside of Helm (by operators, Vault, or database controllers) are not removed by `helm uninstall`.

## License

Apache-2.0
