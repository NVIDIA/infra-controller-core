# Operational Principles

NICo is designed around five foundational principles. These principles inform every architectural decision — from how hardware is monitored to how tenant isolation is enforced.

## The machine is untrustworthy

NICo never relies on software running inside the host OS to make security or isolation decisions. The BlueField DPU is the enforcement boundary. It operates independently of the host and cannot be influenced or compromised by anything running above it. A host that has been tampered with cannot subvert the isolation NICo enforces.

## Operating system requirements are not imposed on the machine

NICo does not require any agents, daemons, or specific configurations inside the host OS. Any operating system installable via iPXE is supported. OS management — patching, upgrades, configuration — is the operator's responsibility. NICo hands off after boot and does not re-enter the host during tenant use.

## After being racked, machines must become ready for use with no human intervention

Discovery, validation, firmware alignment, DPU provisioning, and attestation are fully automated. A newly racked and cabled host should proceed from first power-on to provisioning-ready without requiring a single manual step. Human intervention during bringup is a signal that automation coverage needs to be extended.

## All monitoring of the machine must be done using out-of-band methods

NICo monitors hardware health, firmware state, and machine status exclusively via Redfish and the DPU agent — never via in-band paths that a compromised or unresponsive host OS could influence, block, or spoof. This ensures that monitoring remains reliable regardless of the state of the host.

## The network fabric stays static even during tenancy changes

Leaf switches and routers are not reconfigured when tenants change or when hosts are provisioned and released. Isolation is enforced entirely at the DPU layer (Ethernet via HBN) and via fabric management APIs (InfiniBand via UFM, NVLink via NMX-M). Keeping the physical underlay stable reduces operational risk and simplifies network operations at scale.
