# What NICo Does Not Cover

NICo is deliberately scoped to the infrastructure lifecycle layer. Understanding its boundaries helps set correct expectations when designing an integration or planning a deployment.

## OS and Application Management

NICo boots hosts to a specified image via PXE/iPXE and hands off. Once the OS is running, NICo's responsibility ends at the boot boundary:

- NICo does not run agents or daemons inside the host OS during tenant use
- NICo does not manage OS patching, upgrades, or runtime configuration
- NICo does not install or manage application software, including Kubernetes, SLURM, or storage services
- NICo does not attest the host continuously during tenant use — attestation occurs on ingestion and between tenants, not during active use

OS and workload management above the boot layer is the operator's or ISV's responsibility.

## Cluster Assembly

NICo provisions individual hosts and enforces isolation. It does not assemble or manage clusters:

- NICo does not build or operate SLURM clusters
- NICo does not install or configure Kubernetes on managed hosts
- NICo does not perform workload scheduling or resource allocation within a cluster

Cluster assembly is typically handled by an ISV control plane, BMaaS layer, or orchestration system that consumes NICo's APIs.

## Underlay Network Management

NICo enforces tenant isolation at the DPU layer and via fabric APIs. It does not manage the physical network underlay:

- NICo does not install or configure Cumulus Linux on Ethernet switches
- NICo does not install or manage UFM (UFM is a prerequisite that must be pre-deployed)
- NICo does not manage InfiniBand switches in standalone mode — UFM is required for IB partition management
- NICo does not configure leaf switches, spine switches, or routers — the physical underlay is expected to be stable and pre-configured
- NICo does not integrate with NetQ or other network observability systems

## External Dependencies

NICo depends on several services that must be pre-deployed and are managed externally:

| Dependency | Purpose |
|---|---|
| Kubernetes | Hosts the NICo site controller itself |
| HashiCorp Vault | Certificate management and CSR signing |
| PostgreSQL | State persistence for NICo and site controller components |
| UFM | InfiniBand partition management (required for IB isolation) |
| NMX-M | NVLink partition management (required for NVLink isolation) |

NICo coordinates with these services but does not install, configure, or operate them.

## The Broader Principle

> NICo sits below Kubernetes and platform layers. It exposes clean APIs and events that higher-level platforms can consume. It does not dictate how scheduling, tenancy policy, or workloads are managed above it.

Scheduling logic, tenancy policy enforcement, storage management, and the application stack all belong to the layers above NICo. NICo's job is to make the hardware predictable, repeatable, and safe as a foundation for those layers.
