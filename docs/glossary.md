# NICo Combined Glossary

This glossary consolidates terminology from both halves of NVIDIA Infra Controller (NICo): the on-site Rust control plane, NICo Core, and the cloud-facing Go API layer, NICo REST. It is intended for documentation authors, operators, and anyone reading NICo-related content across the DSX documentation site.

Terms are grouped by domain rather than by repository. A reader looking up "Site Agent" should not need to know whether the term comes from the Core codebase, the REST codebase, or an integration between the two.

This glossary focuses on NICo-specific concepts: terms that only make sense in the context of the NICo platform. Where a term has a general industry definition but carries additional NICo-specific meaning, this glossary explains the NICo-specific part.

<Note> You will encounter references to Forge, Carbide, and BMM in source code, CLI tool names, protobuf definitions, and Helm charts. These were internal NVIDIA project names that preceded the current NICo branding. The rename is largely complete: binary names, Docker images, Helm charts, and Kustomize manifests have been updated from `carbide-rest-*` to `nico-rest-*`, the CLI was renamed from `carbidecli` to `nicocli`, and authorization roles have been migrated from prefixed forms such as `FORGE_PROVIDER_ADMIN` and `NICO_PROVIDER_ADMIN` to canonical suffixes such as `PROVIDER_ADMIN` and `TENANT_ADMIN`. Some legacy names remain in OpenAPI specs and metric namespaces. When this glossary references a binary, role, or configuration key, it uses the current canonical name unless the legacy name is part of the interface.</Note>

## Platform Architecture

### NICo

NVIDIA Infra Controller. NICo is the platform that provides site-local, zero-trust bare-metal lifecycle management with DPU-enforced isolation. It spans NICo Core, the on-site Rust control plane, and NICo REST, the cloud-facing Go API layer.

Related: [What is NICo?](overview/what-is-nico.md), [Architecture Overview](architecture/overview.md)

### NCP

NVIDIA Cloud Partner. In NICo docs, NCP usually refers to an infrastructure provider operating NICo-managed environments for tenant workloads.

Related: [Scope and Boundaries](overview/scope-and-boundaries.md)

### Carbide

A legacy internal name for NICo components. Carbide appears in older source paths, service names, CLI references, and deployment artifacts. New documentation should use NICo names unless referring to an interface that still uses a legacy name.

Related: [What is NICo?](overview/what-is-nico.md)

### AI Factory

A datacenter purpose-built for AI workloads. NICo is the IaaS layer of an AI Factory: it manages the bare-metal lifecycle of every server from rack-and-stack through decommissioning. Everything above NICo, including Kubernetes, GPU Operator, and inference serving, depends on NICo delivering validated, tenant-isolated hardware.

Related: [What is NICo?](overview/what-is-nico.md), [Key Capabilities](overview/capabilities.md), [Scope and Boundaries](overview/scope-and-boundaries.md)

### Hub-and-Spoke Model

The architectural pattern that connects NICo REST, the hub, to one or more datacenters, the spokes. The REST API server, workflow workers, and site manager run centrally, either in the cloud or in a management cluster, while each datacenter runs its own Site Agent alongside a Core instance.

The Site Agent never accepts inbound connections. It initiates outbound connections to Temporal and to the local Core gRPC API. This outbound-only design is critical for datacenters behind firewalls with no inbound connectivity to the management plane.

Related: [Architecture Overview](architecture/overview.md), [Reliable State Handling](architecture/state_handling.md)

### ManagedHost

The fundamental unit of infrastructure that NICo manages. A ManagedHost represents a single physical box in a datacenter and contains exactly two Machines: one DPU and one Host. NICo manages both sides end-to-end: the DPU provides networking enforcement and management infrastructure, while the Host provides the compute resources that tenants consume.

Related: [Managed Host State Machine](architecture/state_machines/managedhost.md), [Ingesting Hosts](provisioning/ingesting-hosts.md)

### Machine

A generic term for either a DPU or a Host. The codebase and APIs use Machine when the distinction between the two does not matter, for example in health reporting, power management, and search queries.

Related: [Health Checks and Health Aggregation](architecture/health_aggregation.md), [Rebooting a Machine](playbooks/machine_reboot.md)

### Host

The compute server as a customer thinks of it, typically an x86-based machine. It is the bare metal that NICo manages. The Host runs whatever operating system the customer provisions onto it. Each Host has its own BMC for out-of-band management.

Related: [Ingesting Hosts](provisioning/ingesting-hosts.md), [Host Validation](provisioning/host-validation.md)

### Instance

A Host that is currently allocated to and being used by a tenant. Instances are the output of the NICo provisioning pipeline: a ready-to-use bare-metal server with validated hardware, tenant-isolated networking, and DHCP and DNS services available.

Instance creation can be done through the gRPC API, where the caller explicitly selects the machine, or through the REST API, which supports resource allocation pools and random selection.

Related: [Day 0 / Day 1 / Day 2 Lifecycle](overview/lifecycle.md), [REST API Reference](/infra-controller/api)

### Leaf

In NICo architecture, the device that a Host connects to for network access. Currently this is a DPU that makes the overlay network available to the tenant. In future iterations, the Leaf might be a specialized switch instead of a DPU.

Related: [Networking Integrations](architecture/networking_integrations.md), [DPU Configuration](architecture/dpu_configuration.md)

### DPU Role in NICo

The DPU is the central enforcement point in NICo architecture. It serves as the VTEP for overlay networking, runs HBN for software-defined networking, and enforces Ethernet tenant isolation in hardware. NICo is responsible for installing the DPU OS and all DPU firmware, including BMC, NIC, and UEFI firmware.

Related: [DPU Configuration](architecture/dpu_configuration.md), [BlueField DPU Operations](dpu-operations.md), [Hardware Compatibility List](hcl.md)

### BlueField

The NVIDIA DPU family used by NICo for tenant isolation and site management. A BlueField card has its own ARM complex, BMC, NIC firmware, and OS image. NICo provisions and manages the BlueField side of each ManagedHost before making the Host available to tenants.

Related: [BlueField DPU Operations](dpu-operations.md), [DPU Configuration](architecture/dpu_configuration.md), [Hardware Compatibility List](hcl.md)

## REST API Services and Binaries

### API Server (`nico-rest-api`)

The main REST API server. It handles external HTTP requests, authenticates callers through JWTs, and routes requests to resource handlers. This is the canonical entry point for tenant, site, machine, and networking operations.

Related: [REST API Reference](/infra-controller/api), [Architecture Overview](architecture/overview.md)

### Workflow Worker (`nico-rest-workflow`)

The Temporal workflow worker service. It registers and executes cloud-level workflow definitions for long-running operations such as site provisioning and hardware lifecycle management. Cloud workflows run centrally and dispatch tasks to site-specific Temporal namespaces.

Related: [Reliable State Handling](architecture/state_handling.md)

### Site Agent (`nico-rest-site-agent`)

The on-site datacenter agent that bridges Temporal workflows to NICo Core. It polls a site-specific Temporal namespace for workflow tasks, translates them into gRPC calls against the local Core instance, and publishes inventory data back through Temporal.

Every provisioning, networking, and lifecycle operation that the REST API dispatches ultimately flows through a Site Agent before reaching hardware. The internal codename Elektra appears throughout the codebase in type names, log prefixes, and Prometheus metric namespaces such as `elektra_site_agent_*`.

Related: [Architecture Overview](architecture/overview.md), [Reliable State Handling](architecture/state_handling.md), [Core Metrics](manuals/metrics/core_metrics.md)

### Site Manager (`nico-rest-site-manager`)

A service responsible for site-level management operations, coordinating between the cloud API layer and on-site infrastructure.

Related: [Architecture Overview](architecture/overview.md)

### Certificate Manager (`nico-rest-cert-manager`)

The native PKI certificate management service. It issues mTLS certificates using Go's `crypto/x509` package and supports SPIFFE certificate issuance through `nico-rest-ca-issuer`.

Related: [TLS and SPIFFE Certificates](development/tls.md), [Re-creating Issuer/CA in Local Dev](development/issuer_ca_recreate.md)

### Database Migrations (`nico-rest-db`)

The schema migration service that manages PostgreSQL database schema evolution for NICo REST.

Related: [Data Model / DB Schema](development/schema.md)

### CLI Client (`nicocli`)

The command-line tool for interacting with the REST API. It supports scripted usage and interactive session management for environment switching and resource commands. It was previously named `carbidecli`.

Related: [REST API Reference](/infra-controller/api)

## Authentication and Authorization

### Authorization Roles

NICo REST defines four canonical authorization roles as unprefixed suffixes: `PROVIDER_ADMIN`, `PROVIDER_VIEWER`, `TENANT_ADMIN`, and `TENANT_VIEWER`. New issuer configuration should use the unprefixed names. Legacy prefixed forms such as `FORGE_PROVIDER_ADMIN`, `NICO_PROVIDER_ADMIN`, and `FORGE_TENANT_ADMIN` are still accepted at runtime for backward compatibility.

| Role | Scope | Capabilities |
| --- | --- | --- |
| `PROVIDER_ADMIN` | Organization, infrastructure provider | Full administrative access to manage sites, hardware, tenants, expected machines, racks, and infrastructure operations |
| `PROVIDER_VIEWER` | Organization, infrastructure provider | Read-only access to infrastructure provider resources such as sites, expected racks, and machines |
| `TENANT_ADMIN` | Organization, tenant | Tenant-scoped administrative access to manage instances, SSH keys, VPC peering, and resources within the assigned tenant organization |
| `TENANT_VIEWER` | Organization, tenant | Read-only access to tenant-scoped resources |

Related: [REST API Reference](/infra-controller/api)

### JWT Claims Processor Pipeline

The chain of processors that extract authorization context from JWT tokens in the REST API. Processor types include Custom, KAS, Keycloak, and SSA. Each processor handles a different token origin and maps claims to internal authorization context.

Related: [REST API Reference](/infra-controller/api)

### Service Account Authentication (SSA)

Machine-to-machine authentication using service account tokens. Service accounts are created per tenant and issue JWTs that bypass interactive Keycloak login flows.

Related: [REST API Reference](/infra-controller/api)

### NGC KAS

NVIDIA GPU Cloud Key Authentication Service. The REST API accepts JWTs issued by NGC KAS and maps NGC organization identities to internal tenant contexts.

Related: [REST API Reference](/infra-controller/api)

### SPIFFE Identity in NICo

NICo uses SPIFFE-based identities for service-to-service authentication within its microservice architecture. The DPU instance metadata service can issue SPIFFE JWT-SVIDs to tenant processes, providing machine identity signed with per-tenant keys.

Related: [TLS and SPIFFE Certificates](development/tls.md)

## Networking

### EVPN

Ethernet VPN. In NICo, EVPN is the control-plane technology used with VXLAN overlays so DPUs and network devices can exchange tenant network reachability information.

Related: [Networking Integrations](architecture/networking_integrations.md), [VPC Network Virtualization](manuals/vpc/vpc_network_virtualization.md)

### VXLAN Overlay Architecture

NICo uses VXLAN as the primary overlay networking technology for Ethernet tenant isolation. The DPU serves as the VTEP, wrapping tenant Ethernet frames in VXLAN headers identified by a VNI. This allows datacenter networking to route IP packets while the x86 Host believes it received an Ethernet frame from a machine on the same local network.

Related: [Networking Integrations](architecture/networking_integrations.md), [VPC Network Virtualization](manuals/vpc/vpc_network_virtualization.md)

### Network Segments

A NICo concept for defining IP address pools. Underlay segments are used for management traffic on the underlying physical network, such as DPU OOB and BMC addresses. Overlay segments are used for tenant-facing networks built on top of VXLAN. NICo assigns IPs from overlay segments to Hosts when creating instances.

Related: [IP Resource Pools](manuals/networking/ip_resource_pools.md), [VNI Resource Pools](manuals/vpc/vni_resource_pools.md)

### HBN in NICo

HBN runs as a container on the DPU and manages network routing using Cumulus Linux components such as FRR and NVUE. NICo installs and manages HBN as part of DPU provisioning. Ethernet tenancy enforcement is performed within HBN on the DPU; NICo does not need to change Spectrum switches running Cumulus Linux.

DPU health reporting includes HBN status such as whether the container is running, BGP peering state, and configuration version.

Related: [DPU Configuration](architecture/dpu_configuration.md), [Health Checks and Health Aggregation](architecture/health_aggregation.md)

### Fabric Nearest Neighbor (FNN)

The networking subsystem within NICo Core that manages VPC creation, subnet allocation, and VXLAN overlay configuration. The acronym appears as Fabric Nearest Neighbor in current configuration documentation. Older design documents expanded the same acronym using a legacy project name.

FNN coordinates DPU-side HBN configuration with the NICo data model to deliver tenant-isolated L2 and L3 networks. FNN supports two VPC virtualization types, `fnn_classic` and `fnn_l3`, and introduces per-VPC routing profiles that control route import and export policies, access tiers, and underlay leak acceptance.

Related: [VPC Network Virtualization](manuals/vpc/vpc_network_virtualization.md), [VPC Routing Profiles](manuals/vpc/vpc_routing_profiles.md), [VPC Peering](manuals/vpc/vpc_peering_management.md)

### DHCP in NICo

NICo runs its own DHCP service. DPUs and Hosts use DHCP to resolve their IP addresses. DHCP relay must be configured on switches connected to DPU OOB interfaces, Host BMCs, and DPU BMCs so requests reach the NICo DHCP service.

NICo issues two IP addresses to the DPU RJ45 port: the DPU OOB address, used for SSH access to the ARM OS and NICo management traffic, and the DPU BMC address, used for Redfish and DPU configuration.

Related: [BMC and Out-of-Band Setup](getting-started/prerequisites/bmc-oob-setup.md), [Network Prerequisites](getting-started/prerequisites/network.md)

### Multi-Tenancy and Isolation

NICo coordinates tenant isolation across four network fabrics, each with its own isolation mechanism.

| Network type | Isolation mechanism | Managed by |
| --- | --- | --- |
| Ethernet north-south | VXLAN with EVPN for VPC creation | DPU through HBN |
| East-west Ethernet | ConnectX-based firmware for VXLAN on ConnectX | Future release |
| InfiniBand | Partition key assignment | UFM |
| NVLink | Partition management | NMX-M |

DPUs enforce Ethernet isolation in hardware, UFM enforces InfiniBand isolation, and NMX-M enforces NVLink isolation, all coordinated by NICo.

Related: [Networking Integrations](architecture/networking_integrations.md), [InfiniBand NIC and Port Selection](architecture/infiniband/nic_selection.md), [NVLink Partitioning](manuals/nvlink_partitioning.md)

### VRF

Virtual Routing and Forwarding. In NICo networking, VRFs provide routing-table isolation for virtual networks so tenant or service routes can be kept separate even when they share physical infrastructure.

Related: [VPC Routing Profiles](manuals/vpc/vpc_routing_profiles.md), [VPC Network Virtualization](manuals/vpc/vpc_network_virtualization.md)

### P_Key

InfiniBand partition key. P_Keys are the isolation mechanism used by UFM for InfiniBand tenant separation, analogous to how VXLAN identifies isolated Ethernet overlays.

Related: [InfiniBand NIC and Port Selection](architecture/infiniband/nic_selection.md)

### NVLink

The high-speed GPU-to-GPU fabric managed outside NICo Core by NMX-M. NICo coordinates with NVLink management so GPU partitioning aligns with tenant isolation.

Related: [NVLink Partitioning](manuals/nvlink_partitioning.md)

### FMDS

Fabric Manager Discovery Service. In NICo docs and operational discussions, FMDS refers to fabric discovery information used to understand site topology and network adjacency.

Related: [Networking Integrations](architecture/networking_integrations.md)

### LLDP

Link Layer Discovery Protocol. NICo uses LLDP-derived network adjacency information to understand how hosts, DPUs, and switches are connected in the site fabric.

Related: [Networking Integrations](architecture/networking_integrations.md), [Ingesting Hosts](provisioning/ingesting-hosts.md)

### Allocation and Constraint

REST API concepts for managing resource assignment to tenants. Allocations bind specific machines or capacity to a tenant. Constraints define rules about what resources a tenant can request, such as specific SKUs or rack locations. Together they control which hardware a tenant can see and consume.

Related: [REST API Reference](/infra-controller/api)

## Boot and Provisioning

### BFB

BlueField bootstream. A BFB is an image format used to install or update the operating system and firmware bundle on a BlueField DPU.

Related: [BlueField DPU Operations](dpu-operations.md), [Bootable Artifacts](bootable_artifacts.md)

### PXE and iPXE Boot

NICo uses PXE and iPXE for network booting. DPUs and Hosts use PXE after startup to install NICo-specific software images as well as tenant-requested images. NICo runs its own PXE server to serve images shipped as part of the software, such as DPU software and iPXE. This PXE server can coexist with other site PXE servers as long as DHCP is configured correctly and the Host can reach the NICo PXE service.

Related: [Bootable Artifacts](bootable_artifacts.md), [Running a PXE Client in a VM](development/vm_pxe_client.md)

### Cloud-Init in NICo

Cloud-init is used in two ways within NICo. DPUs use a NICo-provided cloud-init file to install NICo-related components on top of the base DPU image provided by the NVIDIA networking group. Tenants can provide custom cloud-init configuration to automate installation and configuration of their chosen operating system on the Host.

Related: [Ingesting Hosts](provisioning/ingesting-hosts.md), [BlueField DPU Operations](dpu-operations.md)

### BMC Discovery

NICo discovers BMCs through DHCP. When provisioning a NICo site, operators specify which BMC subnets are on the network fabric. Those subnets must have DHCP relay configured to point to the NICo DHCP service. When a BMC requests an IP address, NICo allocates one and cross-references the MAC address against an expected machine table to look up initial credentials.

Both the Host and the DPU have separate BMCs, so each ManagedHost has two BMCs.

Related: [BMC and Out-of-Band Setup](getting-started/prerequisites/bmc-oob-setup.md), [Ingesting Hosts](provisioning/ingesting-hosts.md)

### Redfish

The HTTP API exposed by BMCs for out-of-band hardware management. NICo uses Redfish to manage power state, credentials, and other BMC-backed operations without relying on the Host operating system.

Related: [Redfish Workflow](architecture/redfish_workflow.md), [Redfish Endpoints Reference](architecture/redfish/endpoints_reference.md)

### OOB

Out of band. OOB management uses a path independent from the Host operating system, usually through BMC and DPU management networks, so NICo can discover, power-cycle, and repair machines even when the tenant OS is unavailable.

Related: [BMC and Out-of-Band Setup](getting-started/prerequisites/bmc-oob-setup.md)

### scout

The discovery service that reports newly discovered DPUs to NICo Core during initial site bring-up. After discovery and provisioning, the DPU-side agent takes over ongoing communication with Core.

Related: [Architecture Overview](architecture/overview.md), [Ingesting Hosts](provisioning/ingesting-hosts.md)

### dpu-agent

The daemon that runs on each DPU after provisioning. It periodically connects to the NICo Core gRPC API to retrieve configuration instructions and report state.

Related: [Architecture Overview](architecture/overview.md), [DPU Configuration](architecture/dpu_configuration.md)

### Managed Host State Machine

The finite state machine that governs the lifecycle of Hosts managed by NICo. A Host progresses through discovery, DPU initialization, host initialization, BOM validation, machine validation, TPM-based attestation measurement, and finally reaches Ready, at which point it can be assigned to an Instance.

The full set of states defined in the `ManagedHostState` enum includes `DpuDiscoveringState`, `DPUInit`, `HostInit`, `BomValidating`, `Validation`, `Measuring`, `PreAssignedMeasuring`, `StartAssignmentCycle`, `Ready`, `Assigned`, `PostAssignedMeasuring`, `WaitingForCleanup`, `HostReprovision`, `DPUReprovision`, `Failed`, `ForceDeletion`, and `Created`. The typical happy-path progression is `DpuDiscoveringState` to `DPUInit` to `HostInit` to `BomValidating` to `Validation` to `Measuring` to `Ready` to `Assigned`.

Related: [Managed Host State Machine](architecture/state_machines/managedhost.md), [Host Validation](provisioning/host-validation.md)

### SKU Validation

The process of verifying that a Machine's actual hardware, or Bill of Materials, matches the expected SKU definition. NICo performs BOM validation during the provisioning pipeline to catch hardware mismatches before a Host reaches Ready.

Related: [SKU Validation](provisioning/sku-validation.md), [Hardware Compatibility List](hcl.md)

## Workflow and Orchestration

### Temporal in NICo

NICo REST uses Temporal as the workflow orchestration engine for long-running operations. Each NICo site gets a dedicated Temporal namespace, providing workflow isolation between sites. Workflows carry authenticated context and use protobuf-encoded payloads.

Related: [Reliable State Handling](architecture/state_handling.md)

### Cloud Workflow and Site Workflow

Two distinct workflow scopes exist. Cloud workflows run in the central management plane and orchestrate cross-site operations. Site workflows run locally at each datacenter and handle site-specific hardware operations. The Site Agent picks up site workflows from its Temporal namespace and translates them into gRPC calls against the local Core instance.

Related: [Reliable State Handling](architecture/state_handling.md)

### Core Proto Synchronization

The protobuf interface shared between NICo Core and NICo REST. Proto definitions originate in Core and are synchronized to REST through a snapshot process. This shared contract defines the gRPC API that the Site Agent uses to communicate with Core.

Related: [Architecture Overview](architecture/overview.md), [Codebase Overview](codebase_overview.md)

### gRPC

The RPC framework used by trusted NICo services to communicate with NICo Core. The Site Agent uses gRPC to translate REST-layer workflows into Core API calls.

Related: [Architecture Overview](architecture/overview.md), [Codebase Overview](codebase_overview.md)

### protobuf

Protocol Buffers. NICo uses protobuf definitions as the shared interface contract for Core APIs and for workflow payloads that move between REST and site-side components.

Related: [Architecture Overview](architecture/overview.md), [Codebase Overview](codebase_overview.md)

## API Patterns

### Resource Handler

The standard CRUD handler pattern used across REST API endpoints. Each resource type, such as sites, machines, instances, fabrics, racks, and tenants, follows the same handler structure with common utilities for pagination, error handling, and model conversion.

Related: [REST API Reference](/infra-controller/api)

### API Data Model and Database Model

The REST API maintains separate model layers. API models define request and response shapes, while database models define PostgreSQL table mappings. Conversion functions bridge the two layers.

Related: [Data Model / DB Schema](development/schema.md), [REST API Reference](/infra-controller/api)

### OpenAPI Specification

The canonical REST API contract. Endpoint additions or modifications require updating the OpenAPI specification. It is validated in CI and used to generate the Go SDK client and rendered API documentation.

Related: [REST API Reference](/infra-controller/api)

## Technology Stack

### Echo v4

The HTTP web framework used for the REST API server. It provides routing, middleware, Prometheus metrics integration, and request handling.

Related: [REST API Reference](/infra-controller/api)

### Bun ORM

The ORM layer used on top of `pgx` for struct-based query building, database migrations, and model mapping between Go structs and PostgreSQL tables.

Related: [Data Model / DB Schema](development/schema.md)

### pgx v5

The PostgreSQL driver providing native protocol support, connection pooling through `pgxpool`, and PostgreSQL-specific type handling. NICo REST uses it underneath Bun for database operations.

Related: [Data Model / DB Schema](development/schema.md)

### Buf

The tool used for Protocol Buffer code generation and management. Proto definitions are sourced from the companion NICo Core repository and synchronized into NICo REST.

Related: [Codebase Overview](codebase_overview.md)

### DOCA

NVIDIA Data Center-on-a-Chip Architecture. In NICo, DOCA is the software framework and release train associated with BlueField DPU functionality that NICo installs and validates.

Related: [DPU Configuration](architecture/dpu_configuration.md), [BlueField DPU Operations](dpu-operations.md)

### UFM

Unified Fabric Manager. NICo relies on UFM for InfiniBand partition management, including assigning P_Keys for tenant isolation on the IB fabric.

Related: [InfiniBand NIC and Port Selection](architecture/infiniband/nic_selection.md)

### IMDS

Instance Metadata Service. In NICo, IMDS can provide tenant workloads with metadata and identity material, including SPIFFE JWT-SVIDs signed with per-tenant keys.

Related: [TLS and SPIFFE Certificates](development/tls.md)

### Connect-RPC

The HTTP-based RPC framework used for the IPAM service's internal communication. Connect-RPC provides protobuf compatibility with HTTP/1.1 and HTTP/2 transports, gRPC health checking, and reflection. The Site Agent communicates with NICo Core over standard gRPC rather than Connect-RPC.

Related: [REST API Reference](/infra-controller/api)

## Health Monitoring

### Health Monitoring

NICo provides hardware health monitoring across both layers. DPUs report health status including HBN configuration correctness and container status, BGP peering state, heartbeat information, configuration version applied, and BMC-side health such as thermal status.

Health information is stored as health report overrides on machine records. The system supports searching for Machines by health alert probe IDs and health alert classifications, allowing API clients to search for health conditions without requiring new API endpoints for each alert category.

Related: [Health Checks and Health Aggregation](architecture/health_aggregation.md), [Health Probe IDs](architecture/health/health_probe_ids.md), [Health Alert Classifications](architecture/health/health_alert_classifications.md)

## Deployment

### Core Deployment

NICo Core commonly runs on a Kubernetes cluster, with three or five control plane nodes recommended. It runs as a set of microservices including API, DNS, DHCP, hardware monitoring, BMC console, and rack management services. Deployment is done through Kubernetes Kustomize manifests.

Related: [Reference Installation](getting-started/installation-options/reference-install.md), [Software Prerequisites](getting-started/prerequisites/software.md)

### REST Deployment

NICo REST is deployed through Helm charts into a Kubernetes cluster. The deployment includes the API server, workflow worker, site manager, database migration job, and Keycloak integration.

Related: [Architecture Overview](architecture/overview.md)

### Disconnected Mode

NICo REST supports operation when the datacenter loses upstream connectivity to the cloud management plane. The Site Agent continues to execute in-flight workflows through its local Temporal connection, and the Core instance continues to manage hardware independently.

Related: [Operational Principles](overview/operational-principles.md), [Reliable State Handling](architecture/state_handling.md)

## Quick Reference: Acronyms

| Acronym | Full name | NICo context |
| --- | --- | --- |
| BGP | Border Gateway Protocol | EVPN route exchange between DPUs and top-of-rack switches |
| BMC | Baseboard Management Controller | Two per ManagedHost, one for the Host and one for the DPU, discovered through DHCP |
| BMM | Bare Metal Manager | Legacy internal name that appears in older source, commands, and docs |
| BOM | Bill of Materials | Hardware inventory validated against expected SKU |
| DHCP | Dynamic Host Configuration Protocol | NICo runs its own DHCP service for BMC and DPU discovery |
| DNS | Domain Name System | NICo runs its own DNS microservice |
| DOCA | Data Center-on-a-Chip Architecture | NVIDIA software framework used by BlueField DPUs |
| DPU | Data Processing Unit | Central enforcement point for tenant isolation |
| EVPN | Ethernet VPN | Control-plane technology used with VXLAN overlays |
| FMDS | Fabric Manager Discovery Service | Fabric discovery term used in site networking and topology contexts |
| FNN | Fabric Nearest Neighbor | VPC, subnet, and VXLAN management subsystem in Core |
| gRPC | Google Remote Procedure Call | RPC framework used between the Site Agent and NICo Core |
| HBN | Host Based Networking | Software networking stack on the DPU for VXLAN and EVPN |
| IMDS | Instance Metadata Service | Service that can issue identity and metadata to tenant processes |
| iPXE | iPXE bootloader | Network bootloader used with PXE workflows |
| KAS | Key Authentication Service | NGC token authentication accepted by REST API |
| LLDP | Link Layer Discovery Protocol | Neighbor discovery protocol used for network topology visibility |
| NCP | NVIDIA Cloud Partner | Infrastructure partner operating NICo-managed environments |
| NICo | NVIDIA Infra Controller | This platform, also known historically as Forge, Carbide, and BMM |
| NMX-M | NVLink Management | NVLink partition management for GPU-to-GPU isolation |
| OOB | Out of Band | Management network path independent from the Host OS |
| P_Key | Partition Key | InfiniBand isolation identifier assigned by UFM |
| protobuf | Protocol Buffers | Interface definition and serialization format for Core APIs |
| PXE | Preboot Execution Environment | Network boot mechanism used by DPUs and Hosts |
| Redfish | Redfish API | BMC API used for out-of-band hardware management |
| RLA | Rack Level Agent | On-site agent for rack switch management |
| SPIFFE | Secure Production Identity Framework for Everyone | Machine identity for service-to-service authentication |
| SSA | Service Account Authentication | Machine-to-machine authentication through per-tenant tokens |
| TPM | Trusted Platform Module | Hardware attestation during Host provisioning |
| UFM | Unified Fabric Manager | InfiniBand partition management for IB isolation |
| VNI | VXLAN Network Identifier | Numeric identifier for a VXLAN overlay network |
| VPC | Virtual Private Cloud | Tenant network boundary, mapped to overlay networking in FNN |
| VRF | Virtual Routing and Forwarding | Routing-table isolation mechanism used in network virtualization |
| VTEP | VXLAN Tunnel Endpoint | DPU role in overlay networking |
| VXLAN | Virtual Extensible LAN | Primary overlay technology for Ethernet isolation |
