# helm-prereqs

Installs the full prerequisite stack for Infra Controller Core and Infra Controller REST on a bare-metal Kubernetes cluster. Everything is orchestrated by a single script:

```bash
export NCX_IMAGE_REGISTRY=<ncx-image-registry>       # unless using --skip-core --skip-rest
export NCX_CORE_IMAGE_TAG=<ncx-core-image-tag>       # unless using --skip-core
export NCX_REST_IMAGE_TAG=<ncx-rest-image-tag>       # unless using --skip-rest
# export REGISTRY_PULL_SECRET=<registry-pull-secret> # optional; authenticated registries only
./setup.sh        # interactive - prompts before deploying Core and REST
./setup.sh -y     # non-interactive - deploys everything
```

## Documentation

For complete step-by-step deployment instructions, see the **[Quick Start Guide](https://nvidia.github.io/ncx-infra-controller-core/documentation/getting-started/quick-start-guide)** in the NVIDIA Infra Controller documentation site. The Quick Start Guide covers:

1. Building NVIDIA Infra Controller containers
2. Preparing the Kubernetes cluster
3. Configuring the site (environment variables, values files, MetalLB, VIPs, preflight)
4. Running `setup.sh`
5. Connecting the OOB network
6. Discovering your first host
7. Verifying the deployment

For manual phase-by-phase installation (re-running individual phases, debugging failures), see the **[Reference Installation](https://nvidia.github.io/ncx-infra-controller-core/documentation/getting-started/installation-options/reference-installation)** guide.

## Directory structure

```
helm-prereqs/
├── setup.sh                    # Main deployment script - runs all phases sequentially
├── preflight.sh                # Pre-flight validation (also run automatically by setup.sh)
├── clean.sh                    # Teardown script - removes everything in reverse order
├── unseal_vault.sh             # Vault init + unseal (called by setup.sh Phase 4)
├── bootstrap_ssh_host_key.sh   # SSH host key generation (called by setup.sh Phase 4)
├── helmfile.yaml               # Helmfile release definitions for all prerequisite components
├── Chart.yaml                  # carbide-prereqs Helm chart metadata
├── values.yaml                 # Top-level values (siteName, PostgreSQL tuning)
├── values/
│   ├── ncx-core.yaml           # Infra Controller Core deployment values (hostname, siteConfig, VIPs)
│   ├── ncx-rest.yaml           # Infra Controller REST deployment values (Keycloak config)
│   ├── ncx-site-agent.yaml     # Site-agent deployment values (DB config, gRPC settings)
│   └── metallb-config.yaml     # MetalLB IP pools, BGP peers, and advertisements
├── templates/                  # carbide-prereqs Helm chart templates (PKI, ESO, PostgreSQL)
├── operators/                  # Raw manifests and operator values (local-path, MetalLB, cert-manager, Vault, ESO)
└── keycloak/                   # Dev Keycloak deployment and token helper scripts
```

## Configuration reference

Before running `setup.sh`, the following values files must be configured for your site. For detailed instructions on each field, see the [Quick Start Guide — Step 3](https://nvidia.github.io/ncx-infra-controller-core/documentation/getting-started/quick-start-guide#step-3--configure-the-site).

### Environment variables

| Variable | Required | Description |
|----------|----------|-------------|
| `KUBECONFIG` | No | Path to your cluster kubeconfig. Optional when the current kubectl context already points at the target cluster. |
| `REGISTRY_PULL_SECRET` | No | NGC API key or pull secret for authenticated image registries. Leave unset for public, preloaded, or externally managed image pulls. |
| `REGISTRY_PULL_USERNAME` | No | Username for generated pull secrets. Defaults to `$oauthtoken`. |
| `NCX_IMAGE_REGISTRY` | Yes, unless `--skip-core --skip-rest` | Base image registry for all Infra Controller images (e.g. `my-registry.example.com/ncx`) |
| `NCX_CORE_IMAGE_TAG` | Yes, unless `--skip-core` | Infra Controller Core image tag (e.g. `v2025.12.30-rc1`) |
| `NCX_REST_IMAGE_TAG` | Yes, unless `--skip-rest` | Infra Controller REST image tag (e.g. `v1.0.4`) |
| `NCX_REPO` | Required unless `--skip-rest` | Path to local clone of `ncx-infra-controller-rest`. Auto-detected from sibling directories. |
| `NCX_SITE_UUID` | No | Stable UUID for this site. Defaults to `a1b2c3d4-e5f6-4000-8000-000000000001`. |
| `PREFLIGHT_CHECK_IMAGE` | No | Image used for preflight per-node checks. Defaults to `busybox:1.36`; set to a local mirror for air-gapped clusters. |

### `values.yaml`

| Key | Default | Must change? | Description |
|-----|---------|-------------|-------------|
| `siteName` | `"TMP_SITE"` | **Yes** | Site identifier, injected into postgres pods as `TMP_SITE` |
| `imagePullSecrets.ngcCarbidePull` | `""` | No (auto) | Pull secret for Infra Controller Core images. Set automatically by `setup.sh` from `REGISTRY_PULL_SECRET` when provided. |
| `vault.nicoCliClientRole.enabled` | `false` | No | Create an optional Vault PKI role for short-lived Infra Controller CLI client certificates. This only defines the certificate profile; issuance access must be granted separately. |
| `vault.nicoCliClientRole.name` | `"nico-cli-client"` | No | Vault role name and certificate `SubjectOU` used to identify Infra Controller CLI client certificates. |
| `vault.nicoCliClientRole.organization` | `""` | No | Optional certificate `SubjectO` value for deployments that want an additional identity marker. |
| `postgresql.instances` | `3` | No | Number of PostgreSQL replicas |
| `postgresql.volumeSize` | `"10Gi"` | No | PVC size per PostgreSQL replica |

### `values/ncx-core.yaml`

| Key | Default | Must change? | Description |
|-----|---------|-------------|-------------|
| `carbide-api.hostname` | `"api-examplesite.example.com"` | **Yes** | External DNS name for the Infra Controller Core API |
| `carbide-api.externalService.annotations...loadBalancerIPs` | `"10.180.126.177"` | **Yes** | MetalLB VIP for carbide-api (from external pool) |
| `siteConfig.sitename` | `"examplesite"` | **Yes** | Short site identifier (must match `siteName` in `values.yaml`) |
| `siteConfig.initial_domain_name` | `"examplesite.example.com"` | **Yes** | Base DNS domain for the site |
| `siteConfig.dhcp_servers` | `["10.180.126.160"]` | **Yes** | DHCP service VIP(s) from your MetalLB internal pool |
| `siteConfig.site_fabric_prefixes` | `["10.180.62.72/29"]` | **Yes** | CIDRs for site fabric (instance-to-instance traffic) |
| `siteConfig.deny_prefixes` | `["10.180.62.64/29", ...]` | **Yes** | CIDRs instances must not reach (OOB, mgmt, underlay) |
| `siteConfig.[pools.lo-ip]` ranges | `{ start = "10.180.62.84", end = "10.180.62.86" }` | **Yes** | Loopback IP range for bare-metal hosts |
| `siteConfig.[pools.vlan-id]` ranges | `{ start = "100", end = "501" }` | **Yes** | VLAN ID allocation range |
| `siteConfig.[pools.vni]` ranges | `{ start = "1024500", end = "1024800" }` | **Yes** | VXLAN Network Identifier range |
| `siteConfig.[networks.admin]` | example values | **Yes** | Admin/OOB network: `prefix` (CIDR), `gateway`, `mtu`, `reserve_first`. `prefix` and `gateway` must not be empty — carbide-api crashes on startup if they are. |
| `siteConfig.[networks.<underlay>]` | `[networks.RNO1-M04-D04-IPMITOR-01]` | **Yes** | One block per underlay data-plane L3 segment: `type = "underlay"`, `prefix`, `gateway`, `mtu`, `reserve_first`. Rename the block to match your site segment name. Add additional blocks for each underlay segment. |
| Per-service `loadBalancerIPs` | example IPs | **Yes** | Stable VIPs for DHCP, DNS, PXE, SSH console, NTP |

### `values/ncx-rest.yaml`

| Key | Default | Must change? | Description |
|-----|---------|-------------|-------------|
| `carbide-rest-api.config.keycloak.enabled` | `true` | No | Use bundled dev Keycloak. Set `false` for BYO IdP. |
| `carbide-rest-api.config.keycloak.baseURL` | `"http://keycloak.carbide-rest:8082"` | For prod | Internal Keycloak URL. Change if using external Keycloak. |
| `carbide-rest-api.config.keycloak.externalBaseURL` | `"http://keycloak.carbide-rest:8082"` | For prod | External Keycloak URL returned in tokens |

### `values/ncx-site-agent.yaml`

| Key | Default | Must change? | Description |
|-----|---------|-------------|-------------|
| `envConfig.DB_ADDR` | `"postgres.postgres.svc.cluster.local"` | For prod | PostgreSQL host address |
| `envConfig.DB_DATABASE` | `"elektratest"` | For prod | Database name |
| `envConfig.DEV_MODE` | `"true"` | For prod | Set to `"false"` in production |
| `envConfig.CARBIDE_SEC_OPT` | `"2"` | No | Security mode: 0=insecure, 1=TLS, 2=mTLS (required) |
| `CLUSTER_ID` | — | No (auto) | Site UUID. Set automatically by `setup.sh` via `--set` from `NCX_SITE_UUID`. |
| `TEMPORAL_SUBSCRIBE_NAMESPACE` | — | No (auto) | Temporal namespace. Set automatically by `setup.sh` via `--set` from `NCX_SITE_UUID`. Must match `CLUSTER_ID`. |

### `values/metallb-config.yaml`

| Key | Default | Must change? | Description |
|-----|---------|-------------|-------------|
| `IPAddressPool (internal).spec.addresses` | `10.180.126.160/28` | **Yes** | Internal VIP CIDR for DHCP, DNS, PXE, SSH, NTP |
| `IPAddressPool (external).spec.addresses` | `10.180.126.176/28` | **Yes** | External VIP CIDR for carbide-api |
| `BGPPeer[*].spec.myASN` | `4244766850` | **Yes** | Cluster-side ASN (same for all nodes) |
| `BGPPeer[*].spec.peerASN` | per-node | **Yes** | TOR router ASN (unique per node) |
| `BGPPeer[*].spec.peerAddress` | per-node | **Yes** | TOR switch IP reachable from each node |
| `BGPPeer[*].spec.nodeSelectors` | example hostnames | **Yes** | Actual node hostnames (`kubectl get nodes`) |
| Advertisement mode | BGP | For dev | For non-BGP environments: comment out BGPPeer/BGPAdvertisement, uncomment L2Advertisement |

## Setup options

`setup.sh` runs preflight validation automatically before making cluster changes.
It supports these common deployment modes:

| Option | Description |
|--------|-------------|
| `-y` | Non-interactive mode; accept setup prompts automatically. |
| `--skip-core` | Install prerequisites and REST, but skip the Infra Controller Core Helm release. |
| `--skip-rest` | Install prerequisites and Core, but skip all REST phases and REST repo checks. |
| `--skip-core --skip-rest` | Infrastructure-only run; image tags, image registry, and REST repo are not required. |
| `--core-values <file>` | Use site-specific Core values instead of `helm-prereqs/values/ncx-core.yaml`. |
| `--metallb-config <path>` | Use a site-specific MetalLB manifest file or kustomize directory. |
| `--site-overlay <dir>` | Apply a site kustomize overlay after Core deploys. |
| `--debug` | Enable bash tracing. This can print secrets, so avoid it in shared logs. |

`REGISTRY_PULL_SECRET` is optional. When it is unset, setup does not create or
inject image pull secrets; images must be public, preloaded, or configured with
existing imagePullSecrets in values.

## What gets deployed

```
local-path-provisioner     (raw manifest - StorageClasses for Vault + PostgreSQL PVCs)
metallb                    (metallb/metallb 0.14.5 - LoadBalancer IPs via BGP or L2)
postgres-operator          (zalando/postgres-operator 1.10.1 - manages forge-pg-cluster)
cert-manager               (jetstack/cert-manager v1.17.1)
vault                      (hashicorp/vault 0.25.0, 3-node HA Raft, TLS)
external-secrets           (external-secrets/external-secrets 0.14.3)
carbide-prereqs            (this Helm chart - forge-system namespace)
Infra Controller Core      (../helm - ncx-core.yaml values)
Infra Controller REST      (ncx-infra-controller-rest/helm/charts/carbide-rest)
  ├── carbide-rest-ca-issuer ClusterIssuer (cert-manager.io)
  ├── postgres StatefulSet  (temporal + keycloak + Infra Controller databases)
  ├── keycloak              (dev OIDC IdP, carbide-dev realm)
  ├── temporal              (temporal-helm/temporal, mTLS)
  ├── carbide-rest          (API, cert-manager, workflow, site-manager)
  └── carbide-rest-site-agent (StatefulSet, bootstrap via site-manager)
```

## Health check

After setup completes, run the read-only health check from the repo root:

```bash
helm-prereqs/health-check.sh
```

The script auto-detects the Core, Vault, Postgres, cert-manager, External
Secrets, and MetalLB namespaces. Override namespace detection if your deployment
uses non-default namespaces:

```bash
CARBIDE_NS=forge-system \
VAULT_NS=vault \
POSTGRES_NS=postgres \
CERT_MANAGER_NS=cert-manager \
ESO_NS=external-secrets \
METALLB_NS=metallb-system \
helm-prereqs/health-check.sh
```

It checks component readiness, Vault and PostgreSQL health, required secrets and
certificates, External Secrets sync status, LoadBalancer VIP assignment, and
basic in-cluster connectivity. Failures exit non-zero; warnings and skipped
probes are reported without failing the run.

## Teardown

```bash
./clean.sh
```

Removes all components in reverse dependency order: Infra Controller REST → Infra Controller Core → helmfile releases → CRDs → namespaces → PVs → local-path-provisioner.
