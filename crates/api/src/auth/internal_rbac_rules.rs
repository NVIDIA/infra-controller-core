/*
 * SPDX-FileCopyrightText: Copyright (c) 2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 * http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use std::collections::HashMap;
use std::sync::LazyLock;

use super::ExternalUserInfo;
use crate::auth::Principal;

static INTERNAL_RBAC_RULES: LazyLock<InternalRBACRules> = LazyLock::new(InternalRBACRules::new);

#[derive(Debug)]
pub struct InternalRBACRules {
    perms: std::collections::HashMap<String, RuleInfo>,
}

#[derive(Debug)]
enum RulePrincipal {
    NicoAdminCLI,
    Machineatron,
    SiteAgent,
    Agent, // Agent on the DPU, NOT site agent
    Scout,
    Dns,
    Dhcp,
    Ssh,
    SshRs,
    Health,
    Pxe,
    Rla,
    MaintenanceJobs,
    DsxExchangeConsumer,
    Anonymous, // Permitted for everything
}
use self::RulePrincipal::{
    Agent, Anonymous, Dhcp, Dns, DsxExchangeConsumer, NicoAdminCLI, Health, Machineatron,
    MaintenanceJobs, Pxe, Rla, Scout, SiteAgent, Ssh, SshRs,
};

impl InternalRBACRules {
    pub fn new() -> Self {
        let mut x = Self {
            perms: HashMap::default(),
        };

        // Add additional permissions to the list below.
        x.perm("Version", vec![Anonymous]);
        x.perm("CreateDomain", vec![]);
        x.perm("CreateDomainLegacy", vec![]);
        x.perm("UpdateDomainLegacy", vec![]);
        x.perm("DeleteDomainLegacy", vec![]);
        x.perm("FindDomainLegacy", vec![NicoAdminCLI]);
        x.perm("UpdateDomain", vec![]);
        x.perm("DeleteDomain", vec![]);
        x.perm("FindDomain", vec![NicoAdminCLI]);
        x.perm("CreateVpc", vec![SiteAgent, Machineatron]);
        x.perm("UpdateVpc", vec![NicoAdminCLI, SiteAgent]);
        x.perm("UpdateVpcVirtualization", vec![NicoAdminCLI, SiteAgent]);
        x.perm("DeleteVpc", vec![Machineatron, SiteAgent]);
        x.perm("FindVpcIds", vec![SiteAgent, NicoAdminCLI, Machineatron]);
        x.perm("FindVpcsByIds", vec![NicoAdminCLI, SiteAgent]);
        x.perm("CreateVpcPrefix", vec![NicoAdminCLI, SiteAgent]);
        x.perm("SearchVpcPrefixes", vec![NicoAdminCLI, SiteAgent]);
        x.perm("GetVpcPrefixes", vec![NicoAdminCLI, SiteAgent]);
        x.perm("UpdateVpcPrefix", vec![NicoAdminCLI, SiteAgent]);
        x.perm("DeleteVpcPrefix", vec![NicoAdminCLI, SiteAgent]);
        x.perm("GetAllDpaInterfaceIds", vec![NicoAdminCLI, SiteAgent]);
        x.perm("FindDpaInterfacesByIds", vec![NicoAdminCLI, SiteAgent]);
        x.perm("CreateDpaInterface", vec![]);
        x.perm("EnsureDpaInterface", vec![]);
        x.perm("DeleteDpaInterface", vec![]);
        x.perm("SetDpaNetworkObservationStatus", vec![]);
        x.perm(
            "FindNetworkSegmentIds",
            vec![NicoAdminCLI, Machineatron, SiteAgent],
        );
        x.perm(
            "FindNetworkSegmentsByIds",
            vec![NicoAdminCLI, Machineatron, SiteAgent],
        );
        x.perm("CreateNetworkSegment", vec![Machineatron, SiteAgent]);
        x.perm(
            "DeleteNetworkSegment",
            vec![NicoAdminCLI, Machineatron, SiteAgent],
        );
        x.perm("NetworkSegmentsForVpc", vec![]);
        x.perm("FindIBPartitionIds", vec![NicoAdminCLI, SiteAgent]);
        x.perm("FindIBPartitionsByIds", vec![NicoAdminCLI, SiteAgent]);
        x.perm("CreateIBPartition", vec![SiteAgent]);
        x.perm("UpdateIBPartition", vec![SiteAgent]);
        x.perm("DeleteIBPartition", vec![SiteAgent]);
        x.perm("IBPartitionsForTenant", vec![]);
        x.perm("FindIBFabricIds", vec![NicoAdminCLI, SiteAgent]);
        x.perm(
            "AllocateInstance",
            vec![NicoAdminCLI, Machineatron, SiteAgent],
        );
        x.perm(
            "AllocateInstances",
            vec![NicoAdminCLI, Machineatron, SiteAgent],
        );
        x.perm("ReleaseInstance", vec![NicoAdminCLI, SiteAgent]);
        x.perm("UpdateInstanceOperatingSystem", vec![SiteAgent]);
        x.perm("UpdateInstanceConfig", vec![NicoAdminCLI, SiteAgent]);
        x.perm("FindInstanceIds", vec![NicoAdminCLI, SiteAgent]);
        x.perm(
            "FindInstancesByIds",
            vec![NicoAdminCLI, SiteAgent, Ssh, SshRs],
        );
        x.perm(
            "FindInstanceByMachineID",
            vec![NicoAdminCLI, Agent, SiteAgent],
        );
        x.perm("RecordObservedInstanceNetworkStatus", vec![]);
        x.perm(
            "GetManagedHostNetworkConfig",
            vec![NicoAdminCLI, Agent, Machineatron, SiteAgent],
        );
        x.perm("RecordDpuNetworkStatus", vec![Agent, Machineatron]);
        x.perm(
            "ListMachineHealthReports",
            vec![NicoAdminCLI, Health, Ssh, SshRs],
        );
        x.perm(
            "InsertMachineHealthReport",
            vec![NicoAdminCLI, Health, Ssh, SshRs, Rla],
        );
        x.perm(
            "RemoveMachineHealthReport",
            vec![NicoAdminCLI, Health, Ssh, SshRs, Rla],
        );
        x.perm(
            "ListRackHealthReports",
            vec![NicoAdminCLI, Health, DsxExchangeConsumer],
        );
        x.perm(
            "InsertRackHealthReport",
            vec![NicoAdminCLI, Health, DsxExchangeConsumer],
        );
        x.perm(
            "RemoveRackHealthReport",
            vec![NicoAdminCLI, Health, DsxExchangeConsumer],
        );
        x.perm("ListSwitchHealthReports", vec![NicoAdminCLI, Health]);
        x.perm("InsertSwitchHealthReport", vec![NicoAdminCLI, Health]);
        x.perm("RemoveSwitchHealthReport", vec![NicoAdminCLI, Health]);
        x.perm("ListPowerShelfHealthReports", vec![NicoAdminCLI, Health]);
        x.perm("InsertPowerShelfHealthReport", vec![NicoAdminCLI, Health]);
        x.perm("RemovePowerShelfHealthReport", vec![NicoAdminCLI, Health]);
        // Deprecated aliases for the machine health report RPCs. Mirror the
        // permissions of their canonical equivalents above. Drop once we're
        // confident no clients are still calling the old names.
        x.perm(
            "ListHealthReportOverrides",
            vec![NicoAdminCLI, Health, Ssh, SshRs],
        );
        x.perm(
            "InsertHealthReportOverride",
            vec![NicoAdminCLI, Health, Ssh, SshRs, Rla],
        );
        x.perm(
            "RemoveHealthReportOverride",
            vec![NicoAdminCLI, Health, Ssh, SshRs, Rla],
        );
        x.perm("DpuAgentUpgradeCheck", vec![Scout]);
        x.perm("DpuAgentUpgradePolicyAction", vec![NicoAdminCLI]);
        x.perm("LookupRecord", vec![Dns]);
        x.perm("LookupRecordLegacy", vec![Dns]);
        x.perm("GetAllDomainMetadata", vec![Dns]);
        x.perm("GetAllDomains", vec![Dns]);
        x.perm("InvokeInstancePower", vec![NicoAdminCLI, SiteAgent]);
        x.perm("NicoAgentControl", vec![Machineatron, Scout]);
        x.perm("DiscoverMachine", vec![Anonymous]);
        x.perm("RenewMachineCertificate", vec![Agent]);
        x.perm("DiscoveryCompleted", vec![Machineatron, Scout]);
        x.perm("CleanupMachineCompleted", vec![Machineatron, Scout]);
        x.perm("ReportNicoScoutError", vec![Scout]);
        x.perm("ReportScoutFirmwareUpgradeStatus", vec![Scout]);
        x.perm("DiscoverDhcp", vec![Dhcp, Machineatron]);
        x.perm("ExpireDhcpLease", vec![Dhcp, Machineatron]);
        x.perm("AssignStaticAddress", vec![NicoAdminCLI]);
        x.perm("RemoveStaticAddress", vec![NicoAdminCLI]);
        x.perm("FindInterfaceAddresses", vec![NicoAdminCLI]);
        x.perm("FindInterfaces", vec![NicoAdminCLI, Agent, Rla]);
        x.perm("DeleteInterface", vec![NicoAdminCLI]);
        x.perm("FindIpAddress", vec![NicoAdminCLI]);
        x.perm(
            "FindMachineIds",
            vec![
                NicoAdminCLI,
                Machineatron,
                Health,
                SiteAgent,
                Ssh,
                SshRs,
                Rla,
            ],
        );
        x.perm(
            "FindMachinesByIds",
            vec![
                NicoAdminCLI,
                Machineatron,
                Health,
                SiteAgent,
                Ssh,
                SshRs,
                Rla,
            ],
        );
        x.perm("FindConnectedDevicesByDpuMachineIds", vec![NicoAdminCLI]);
        x.perm("FindMachineIdsByBmcIps", vec![NicoAdminCLI, Rla]);
        x.perm("FindMachineHealthHistories", vec![NicoAdminCLI, SiteAgent]);
        x.perm("FindMachineStateHistories", vec![NicoAdminCLI, SiteAgent]);
        x.perm("IdentifyUuid", vec![NicoAdminCLI]);
        x.perm("IdentifyMac", vec![NicoAdminCLI]);
        x.perm("IdentifySerial", vec![NicoAdminCLI, Machineatron, Rla]);
        x.perm("GetBMCMetaData", vec![Health, Ssh, SshRs]);
        x.perm("UpdateBMCMetaData", vec![Machineatron]);
        x.perm("UpdateMachineCredentials", vec![]);
        x.perm("GetPxeInstructions", vec![Pxe, Machineatron]);
        x.perm("GetCloudInitInstructions", vec![Pxe]);
        x.perm("Echo", vec![Dhcp]);
        x.perm("CreateTenant", vec![SiteAgent]);
        x.perm("FindTenant", vec![SiteAgent, NicoAdminCLI]);
        x.perm("UpdateTenant", vec![SiteAgent, NicoAdminCLI]);
        x.perm("CreateTenantKeyset", vec![SiteAgent]);
        x.perm("FindTenantKeysetIds", vec![NicoAdminCLI, SiteAgent]);
        x.perm("FindTenantKeysetsByIds", vec![NicoAdminCLI, SiteAgent]);
        x.perm("UpdateTenantKeyset", vec![SiteAgent]);
        x.perm("DeleteTenantKeyset", vec![SiteAgent]);
        x.perm("ValidateTenantPublicKey", vec![SiteAgent, Ssh, SshRs]);
        x.perm("GetBmcCredentials", vec![Health]);
        x.perm("GetAllManagedHostNetworkStatus", vec![NicoAdminCLI]);
        x.perm(
            "GetSiteExplorationReport",
            vec![NicoAdminCLI, Machineatron],
        );
        x.perm("ClearSiteExplorationError", vec![NicoAdminCLI]);
        x.perm("IsBmcInManagedHost", vec![NicoAdminCLI]);
        x.perm("Explore", vec![NicoAdminCLI, Rla]);
        x.perm("ReExploreEndpoint", vec![NicoAdminCLI, Rla]);
        x.perm("DeleteExploredEndpoint", vec![NicoAdminCLI]);
        x.perm("PauseExploredEndpointRemediation", vec![NicoAdminCLI]);
        x.perm("FindExploredEndpointIds", vec![NicoAdminCLI, Rla]);
        x.perm("FindExploredEndpointsByIds", vec![NicoAdminCLI, Rla]);
        x.perm("FindExploredManagedHostIds", vec![NicoAdminCLI, Rla]);
        x.perm("FindExploredManagedHostsByIds", vec![NicoAdminCLI, Rla]);
        x.perm("AdminForceDeleteMachine", vec![NicoAdminCLI, Machineatron]);
        x.perm("AdminForceDeleteSwitch", vec![NicoAdminCLI, Machineatron]);
        x.perm(
            "AdminForceDeletePowerShelf",
            vec![NicoAdminCLI, Machineatron],
        );
        x.perm("AdminListResourcePools", vec![NicoAdminCLI]);
        x.perm("AdminGrowResourcePool", vec![NicoAdminCLI]);
        x.perm("SetMaintenance", vec![NicoAdminCLI, SiteAgent, Rla]);
        x.perm("SetDynamicConfig", vec![NicoAdminCLI, Machineatron]);
        x.perm("TriggerDpuReprovisioning", vec![NicoAdminCLI]);
        x.perm("TriggerHostReprovisioning", vec![NicoAdminCLI, Rla]);
        x.perm("ListDpuWaitingForReprovisioning", vec![NicoAdminCLI]);
        x.perm("MarkManualFirmwareUpgradeComplete", vec![NicoAdminCLI]);
        x.perm(
            "ListHostsWaitingForReprovisioning",
            vec![NicoAdminCLI, Rla],
        );
        x.perm("GetDpuInfoList", vec![Agent]);
        x.perm("GetMachineBootOverride", vec![NicoAdminCLI]);
        x.perm("SetMachineBootOverride", vec![NicoAdminCLI]);
        x.perm("ClearMachineBootOverride", vec![NicoAdminCLI]);
        x.perm("GetNetworkTopology", vec![NicoAdminCLI]);
        x.perm("FindNetworkDevicesByDeviceIds", vec![NicoAdminCLI]);
        x.perm("CreateCredential", vec![NicoAdminCLI]);
        x.perm("DeleteCredential", vec![NicoAdminCLI]);
        x.perm("GetRouteServers", vec![NicoAdminCLI]);
        x.perm("AddRouteServers", vec![NicoAdminCLI]);
        x.perm("RemoveRouteServers", vec![NicoAdminCLI]);
        x.perm("ReplaceRouteServers", vec![]);
        x.perm("UpdateAgentReportedInventory", vec![Agent]);
        x.perm("UpdateInstancePhoneHomeLastContact", vec![Agent]);
        x.perm("SetHostUefiPassword", vec![NicoAdminCLI]);
        x.perm("ClearHostUefiPassword", vec![NicoAdminCLI]);
        x.perm("AddExpectedMachine", vec![NicoAdminCLI, SiteAgent, Rla]);
        x.perm("DeleteExpectedMachine", vec![NicoAdminCLI, SiteAgent]);
        x.perm("UpdateExpectedMachine", vec![NicoAdminCLI, SiteAgent]);
        x.perm("CreateExpectedMachines", vec![NicoAdminCLI, SiteAgent]);
        x.perm("UpdateExpectedMachines", vec![NicoAdminCLI, SiteAgent]);
        x.perm("GetExpectedMachine", vec![NicoAdminCLI, Rla]);
        x.perm(
            "GetAllExpectedMachines",
            vec![NicoAdminCLI, SiteAgent, Rla],
        );
        x.perm("ReplaceAllExpectedMachines", vec![NicoAdminCLI]);
        x.perm("DeleteAllExpectedMachines", vec![NicoAdminCLI]);
        x.perm(
            "GetAllExpectedMachinesLinked",
            vec![NicoAdminCLI, SiteAgent, Rla],
        );
        x.perm(
            "GetAllUnexpectedMachines",
            vec![NicoAdminCLI, SiteAgent, Rla],
        );
        x.perm("AttestQuote", vec![Anonymous]);
        x.perm("SignMachineIdentity", vec![Agent]);
        x.perm(
            "GetTenantIdentityConfiguration",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm(
            "SetTenantIdentityConfiguration",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm(
            "DeleteTenantIdentityConfiguration",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm("GetTokenDelegation", vec![NicoAdminCLI, SiteAgent]);
        x.perm("SetTokenDelegation", vec![NicoAdminCLI, SiteAgent]);
        x.perm("DeleteTokenDelegation", vec![NicoAdminCLI, SiteAgent]);
        x.perm("GetJWKS", vec![Anonymous, Agent, NicoAdminCLI, SiteAgent]);
        x.perm(
            "GetOpenIDConfiguration",
            vec![Anonymous, Agent, NicoAdminCLI, SiteAgent],
        );
        x.perm("CreateMeasurementBundle", vec![NicoAdminCLI, SiteAgent]);
        x.perm("DeleteMeasurementBundle", vec![NicoAdminCLI, SiteAgent]);
        x.perm("RenameMeasurementBundle", vec![NicoAdminCLI, SiteAgent]);
        x.perm("UpdateMeasurementBundle", vec![NicoAdminCLI, SiteAgent]);
        x.perm("ShowMeasurementBundle", vec![NicoAdminCLI]);
        x.perm("ShowMeasurementBundles", vec![NicoAdminCLI]);
        x.perm("ListMeasurementBundles", vec![NicoAdminCLI]);
        x.perm("ListMeasurementBundleMachines", vec![NicoAdminCLI]);
        x.perm("FindClosestBundleMatch", vec![NicoAdminCLI]);
        x.perm("DeleteMeasurementJournal", vec![NicoAdminCLI]);
        x.perm("ShowMeasurementJournal", vec![NicoAdminCLI]);
        x.perm("ShowMeasurementJournals", vec![NicoAdminCLI]);
        x.perm("ListMeasurementJournal", vec![NicoAdminCLI]);
        x.perm("AttestCandidateMachine", vec![NicoAdminCLI]);
        x.perm("ShowCandidateMachine", vec![NicoAdminCLI]);
        x.perm("ShowCandidateMachines", vec![NicoAdminCLI]);
        x.perm("ListCandidateMachines", vec![NicoAdminCLI]);
        x.perm(
            "CreateMeasurementSystemProfile",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm(
            "DeleteMeasurementSystemProfile",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm(
            "RenameMeasurementSystemProfile",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm("ShowMeasurementSystemProfile", vec![NicoAdminCLI]);
        x.perm("ShowMeasurementSystemProfiles", vec![NicoAdminCLI]);
        x.perm("ListMeasurementSystemProfiles", vec![NicoAdminCLI]);
        x.perm("ListMeasurementSystemProfileBundles", vec![NicoAdminCLI]);
        x.perm("ListMeasurementSystemProfileMachines", vec![NicoAdminCLI]);
        x.perm("CreateMeasurementReport", vec![NicoAdminCLI, SiteAgent]);
        x.perm("DeleteMeasurementReport", vec![NicoAdminCLI, SiteAgent]);
        x.perm("PromoteMeasurementReport", vec![NicoAdminCLI, SiteAgent]);
        x.perm("RevokeMeasurementReport", vec![NicoAdminCLI, SiteAgent]);
        x.perm("ShowMeasurementReportForId", vec![NicoAdminCLI]);
        x.perm("ShowMeasurementReportsForMachine", vec![NicoAdminCLI]);
        x.perm("ShowMeasurementReports", vec![NicoAdminCLI]);
        x.perm("ListMeasurementReport", vec![NicoAdminCLI]);
        x.perm("MatchMeasurementReport", vec![NicoAdminCLI]);
        x.perm("ImportSiteMeasurements", vec![NicoAdminCLI, SiteAgent]);
        x.perm("ExportSiteMeasurements", vec![NicoAdminCLI, SiteAgent]);
        x.perm(
            "AddMeasurementTrustedMachine",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm(
            "RemoveMeasurementTrustedMachine",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm(
            "AddMeasurementTrustedProfile",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm(
            "RemoveMeasurementTrustedProfile",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm(
            "ListMeasurementTrustedMachines",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm(
            "ListMeasurementTrustedProfiles",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm("ListAttestationSummary", vec![SiteAgent]);
        x.perm("ImportStorageCluster", vec![NicoAdminCLI, SiteAgent]);
        x.perm("DeleteStorageCluster", vec![NicoAdminCLI, SiteAgent]);
        x.perm("ListStorageCluster", vec![NicoAdminCLI, SiteAgent]);
        x.perm("GetStorageCluster", vec![NicoAdminCLI, SiteAgent]);
        x.perm("UpdateStorageCluster", vec![NicoAdminCLI, SiteAgent]);
        x.perm("CreateStoragePool", vec![NicoAdminCLI, SiteAgent]);
        x.perm("DeleteStoragePool", vec![NicoAdminCLI, SiteAgent]);
        x.perm("ListStoragePool", vec![NicoAdminCLI, SiteAgent]);
        x.perm("GetStoragePool", vec![NicoAdminCLI, SiteAgent]);
        x.perm("UpdateStoragePool", vec![NicoAdminCLI, SiteAgent]);
        x.perm("CreateStorageVolume", vec![NicoAdminCLI, SiteAgent]);
        x.perm("DeleteStorageVolume", vec![NicoAdminCLI, SiteAgent]);
        x.perm("ListStorageVolume", vec![NicoAdminCLI, SiteAgent]);
        x.perm("GetStorageVolume", vec![NicoAdminCLI, SiteAgent]);
        x.perm("UpdateStorageVolume", vec![NicoAdminCLI, SiteAgent]);
        x.perm("CreateOsImage", vec![NicoAdminCLI, SiteAgent]);
        x.perm("DeleteOsImage", vec![NicoAdminCLI, SiteAgent]);
        x.perm("ListOsImage", vec![NicoAdminCLI, SiteAgent]);
        x.perm("GetOsImage", vec![NicoAdminCLI, SiteAgent]);
        x.perm("UpdateOsImage", vec![NicoAdminCLI, SiteAgent]);
        x.perm("CreateOperatingSystem", vec![NicoAdminCLI, SiteAgent]);
        x.perm("GetOperatingSystem", vec![NicoAdminCLI, SiteAgent]);
        x.perm("UpdateOperatingSystem", vec![NicoAdminCLI, SiteAgent]);
        x.perm("DeleteOperatingSystem", vec![NicoAdminCLI, SiteAgent]);
        x.perm("FindOperatingSystemIds", vec![NicoAdminCLI, SiteAgent]);
        x.perm("FindOperatingSystemsByIds", vec![NicoAdminCLI, SiteAgent]);
        x.perm(
            "GetOperatingSystemCachableIpxeTemplateArtifacts",
            vec![NicoAdminCLI],
        );
        x.perm(
            "UpdateOperatingSystemCachableIpxeTemplateArtifacts",
            vec![NicoAdminCLI],
        );
        x.perm("GetIpxeTemplate", vec![NicoAdminCLI, SiteAgent]);
        x.perm("ListIpxeTemplates", vec![NicoAdminCLI, SiteAgent]);
        x.perm("CreateRackFirmware", vec![NicoAdminCLI]);
        x.perm("DeleteRackFirmware", vec![NicoAdminCLI]);
        x.perm("FindRackStateHistories", vec![NicoAdminCLI, Machineatron]);
        x.perm("ListRackFirmware", vec![NicoAdminCLI]);
        x.perm("GetRackFirmware", vec![NicoAdminCLI]);
        x.perm("ApplyRackFirmware", vec![NicoAdminCLI]);
        x.perm("GetRackFirmwareJobStatus", vec![NicoAdminCLI]);
        x.perm("GetRackFirmwareHistory", vec![NicoAdminCLI]);
        x.perm("RackFirmwareSetDefault", vec![NicoAdminCLI]);
        x.perm("RebootCompleted", vec![Machineatron, Scout]);
        x.perm("PersistValidationResult", vec![Scout]);
        x.perm("GetMachineValidationResults", vec![NicoAdminCLI, Scout]);
        x.perm("MachineValidationCompleted", vec![Machineatron, Scout]);
        x.perm("MachineSetAutoUpdate", vec![NicoAdminCLI, Rla]);
        x.perm(
            "GetMachineValidationExternalConfig",
            vec![NicoAdminCLI, Scout],
        );
        x.perm(
            "AddUpdateMachineValidationExternalConfig",
            vec![NicoAdminCLI],
        );
        x.perm("GetMachineValidationRuns", vec![NicoAdminCLI]);
        x.perm("AdminBmcReset", vec![NicoAdminCLI]);
        x.perm("AdminPowerControl", vec![NicoAdminCLI, Rla]);
        x.perm("DisableSecureBoot", vec![NicoAdminCLI]);
        x.perm("MachineSetup", vec![NicoAdminCLI]);
        x.perm("SetDpuFirstBootOrder", vec![NicoAdminCLI]);
        x.perm("OnDemandMachineValidation", vec![NicoAdminCLI]);
        x.perm("OnDemandRackMaintenance", vec![NicoAdminCLI]);
        x.perm("TpmAddCaCert", vec![NicoAdminCLI, SiteAgent]);
        x.perm("TpmShowCaCerts", vec![NicoAdminCLI, SiteAgent]);
        x.perm("TpmShowUnmatchedEkCerts", vec![NicoAdminCLI, SiteAgent]);
        x.perm("TpmDeleteCaCert", vec![NicoAdminCLI, SiteAgent]);
        x.perm("RedfishListActions", vec![NicoAdminCLI]);
        x.perm("RedfishCreateAction", vec![NicoAdminCLI]);
        x.perm("RedfishApproveAction", vec![NicoAdminCLI]);
        x.perm("RedfishApplyAction", vec![NicoAdminCLI]);
        x.perm("RedfishCancelAction", vec![NicoAdminCLI]);
        x.perm("FindTenantOrganizationIds", vec![SiteAgent, NicoAdminCLI]);
        x.perm(
            "FindTenantsByOrganizationIds",
            vec![SiteAgent, NicoAdminCLI],
        );
        x.perm("FindMacAddressByBmcIp", vec![SiteAgent]);
        x.perm("BmcCredentialStatus", vec![NicoAdminCLI, SiteAgent]);
        x.perm(
            "GetMachineValidationExternalConfigs",
            vec![NicoAdminCLI, Scout, SiteAgent],
        );
        x.perm(
            "RemoveMachineValidationExternalConfig",
            vec![NicoAdminCLI, Scout, SiteAgent],
        );
        x.perm(
            "GetMachineValidationTests",
            vec![NicoAdminCLI, SiteAgent, Agent, Scout],
        );
        x.perm("AddMachineValidationTest", vec![NicoAdminCLI, SiteAgent]);
        x.perm(
            "UpdateMachineValidationTest",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm(
            "MachineValidationTestVerfied",
            vec![NicoAdminCLI, Scout, SiteAgent],
        );
        x.perm(
            "MachineValidationTestNextVersion",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm(
            "MachineValidationTestEnableDisableTest",
            vec![NicoAdminCLI, SiteAgent, Scout],
        );
        x.perm("UpdateMachineValidationRun", vec![Scout, SiteAgent]);
        x.perm("FindInstanceTypeIds", vec![SiteAgent, NicoAdminCLI]);
        x.perm("FindInstanceTypesByIds", vec![NicoAdminCLI, SiteAgent]);
        x.perm("CreateInstanceType", vec![NicoAdminCLI, SiteAgent]);
        x.perm("UpdateInstanceType", vec![NicoAdminCLI, SiteAgent]);
        x.perm("DeleteInstanceType", vec![NicoAdminCLI, SiteAgent]);
        x.perm(
            "AssociateMachinesWithInstanceType",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm(
            "RemoveMachineInstanceTypeAssociation",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm("RedfishBrowse", vec![NicoAdminCLI]);
        x.perm("UfmBrowse", vec![NicoAdminCLI]);
        x.perm("NmxmBrowse", vec![NicoAdminCLI]);
        x.perm("UpdateMachineMetadata", vec![NicoAdminCLI, SiteAgent]);
        x.perm("UpdateRackMetadata", vec![NicoAdminCLI, SiteAgent]);
        x.perm("UpdateSwitchMetadata", vec![NicoAdminCLI, SiteAgent]);
        x.perm("UpdatePowerShelfMetadata", vec![NicoAdminCLI, SiteAgent]);
        x.perm("CreateNetworkSecurityGroup", vec![NicoAdminCLI, SiteAgent]);
        x.perm(
            "FindNetworkSecurityGroupIds",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm(
            "FindNetworkSecurityGroupsByIds",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm("UpdateNetworkSecurityGroup", vec![NicoAdminCLI, SiteAgent]);
        x.perm("DeleteNetworkSecurityGroup", vec![NicoAdminCLI, SiteAgent]);
        x.perm(
            "GetNetworkSecurityGroupPropagationStatus",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm(
            "GetNetworkSecurityGroupAttachments",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm(
            "GetDesiredFirmwareVersions",
            vec![NicoAdminCLI, Machineatron, Rla],
        );
        x.perm("CreateSku", vec![NicoAdminCLI]);
        x.perm("GenerateSkuFromMachine", vec![NicoAdminCLI]);
        x.perm("AssignSkuToMachine", vec![NicoAdminCLI]);
        x.perm("VerifySkuForMachine", vec![NicoAdminCLI]);
        x.perm("RemoveSkuAssociation", vec![NicoAdminCLI]);
        x.perm("GetAllSkuIds", vec![NicoAdminCLI, SiteAgent, Rla]);
        x.perm("FindSkusByIds", vec![NicoAdminCLI, SiteAgent, Rla]);
        x.perm("DeleteSku", vec![NicoAdminCLI]);
        x.perm("UpdateSkuMetadata", vec![NicoAdminCLI]);
        x.perm("UpdateMachineHardwareInfo", vec![NicoAdminCLI]);
        x.perm("ReplaceSku", vec![NicoAdminCLI]);
        x.perm(
            "GetManagedHostQuarantineState",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm(
            "SetManagedHostQuarantineState",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm(
            "ClearManagedHostQuarantineState",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm("CreateVpcPeering", vec![NicoAdminCLI]);
        x.perm("FindVpcPeeringIds", vec![NicoAdminCLI]);
        x.perm("FindVpcPeeringsByIds", vec![NicoAdminCLI]);
        x.perm("DeleteVpcPeering", vec![NicoAdminCLI]);
        x.perm("ResetHostReprovisioning", vec![NicoAdminCLI, Rla]);
        x.perm("CopyBfbToDpuRshim", vec![NicoAdminCLI]);
        x.perm("GetPowerOptions", vec![NicoAdminCLI, SiteAgent, Rla]);
        x.perm("UpdatePowerOption", vec![NicoAdminCLI, SiteAgent, Rla]);
        x.perm("CreateBmcUser", vec![NicoAdminCLI]);
        x.perm("DeleteBmcUser", vec![NicoAdminCLI]);
        x.perm("SetFirmwareUpdateTimeWindow", vec![NicoAdminCLI, Rla]);
        x.perm("ListHostFirmware", vec![NicoAdminCLI, Rla]);
        x.perm("EnableInfiniteBoot", vec![NicoAdminCLI]);
        x.perm("IsInfiniteBootEnabled", vec![NicoAdminCLI]);
        x.perm("Lockdown", vec![NicoAdminCLI]);
        x.perm("LockdownStatus", vec![NicoAdminCLI]);
        x.perm(
            "PublishMlxDeviceReport",
            vec![Agent, Scout, Machineatron, NicoAdminCLI],
        );
        x.perm(
            "PublishMlxObservationReport",
            vec![Agent, Scout, Machineatron, NicoAdminCLI],
        );
        x.perm("TrimTable", vec![NicoAdminCLI, MaintenanceJobs]);
        x.perm("CreateRemediation", vec![NicoAdminCLI]);
        x.perm("ApproveRemediation", vec![NicoAdminCLI]);
        x.perm("RevokeRemediation", vec![NicoAdminCLI]);
        x.perm("EnableRemediation", vec![NicoAdminCLI]);
        x.perm("DisableRemediation", vec![NicoAdminCLI]);
        x.perm("FindRemediationIds", vec![NicoAdminCLI]);
        x.perm("FindRemediationsByIds", vec![NicoAdminCLI]);
        x.perm("FindAppliedRemediations", vec![NicoAdminCLI]);
        x.perm("FindAppliedRemediationIds", vec![NicoAdminCLI]);
        x.perm("GetNextRemediationForMachine", vec![Agent]);
        x.perm("RemediationApplied", vec![Agent]);
        x.perm("DetermineMachineIngestionState", vec![NicoAdminCLI, Rla]);
        x.perm("AllowIngestionAndPowerOn", vec![NicoAdminCLI, Rla]);
        x.perm("SetPrimaryDpu", vec![NicoAdminCLI]);
        x.perm("CreateDpuExtensionService", vec![NicoAdminCLI, SiteAgent]);
        x.perm("UpdateDpuExtensionService", vec![NicoAdminCLI, SiteAgent]);
        x.perm("DeleteDpuExtensionService", vec![NicoAdminCLI, SiteAgent]);
        x.perm("FindDpuExtensionServiceIds", vec![NicoAdminCLI, SiteAgent]);
        x.perm(
            "FindDpuExtensionServicesByIds",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm(
            "GetDpuExtensionServiceVersionsInfo",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm(
            "FindInstancesByDpuExtensionService",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm("TriggerMachineAttestation", vec![NicoAdminCLI, SiteAgent]);
        x.perm("CancelMachineAttestation", vec![NicoAdminCLI, SiteAgent]);
        x.perm(
            "FindMachineIdsUnderAttestation",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm(
            "FindMachinesUnderAttestation",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm("FindPowerShelves", vec![NicoAdminCLI, Machineatron, Rla]);
        x.perm("FindPowerShelfIds", vec![NicoAdminCLI, Machineatron, Rla]);
        x.perm(
            "FindPowerShelvesByIds",
            vec![NicoAdminCLI, Machineatron, Rla],
        );
        x.perm("CreatePowerShelf", vec![NicoAdminCLI, Machineatron]);
        x.perm("DeletePowerShelf", vec![NicoAdminCLI, Machineatron]);
        x.perm(
            "AddExpectedPowerShelf",
            vec![NicoAdminCLI, Machineatron, Rla],
        );
        x.perm(
            "DeleteExpectedPowerShelf",
            vec![NicoAdminCLI, Machineatron],
        );
        x.perm(
            "UpdateExpectedPowerShelf",
            vec![NicoAdminCLI, Machineatron],
        );
        x.perm(
            "GetExpectedPowerShelf",
            vec![NicoAdminCLI, Machineatron, Rla],
        );
        x.perm(
            "GetAllExpectedPowerShelves",
            vec![NicoAdminCLI, Machineatron, SiteAgent, Rla],
        );
        x.perm(
            "ReplaceAllExpectedPowerShelves",
            vec![NicoAdminCLI, Machineatron],
        );
        x.perm(
            "DeleteAllExpectedPowerShelves",
            vec![NicoAdminCLI, Machineatron],
        );
        x.perm(
            "GetAllExpectedPowerShelvesLinked",
            vec![NicoAdminCLI, Machineatron, SiteAgent, Rla],
        );
        x.perm(
            "FindPowerShelfStateHistories",
            vec![NicoAdminCLI, Machineatron, Rla],
        );
        x.perm(
            "FindSwitches",
            vec![NicoAdminCLI, Machineatron, Rla, Health],
        );
        x.perm(
            "FindSwitchIds",
            vec![NicoAdminCLI, Machineatron, Rla, Health],
        );
        x.perm(
            "FindSwitchesByIds",
            vec![NicoAdminCLI, Machineatron, Rla, Health],
        );
        x.perm("CreateSwitch", vec![NicoAdminCLI, Machineatron]);
        x.perm("DeleteSwitch", vec![NicoAdminCLI, Machineatron]);
        x.perm("AddExpectedSwitch", vec![NicoAdminCLI, Machineatron, Rla]);
        x.perm("DeleteExpectedSwitch", vec![NicoAdminCLI, Machineatron]);
        x.perm("UpdateExpectedSwitch", vec![NicoAdminCLI, Machineatron]);
        x.perm("GetExpectedSwitch", vec![NicoAdminCLI, Machineatron, Rla]);
        x.perm(
            "GetAllExpectedSwitches",
            vec![NicoAdminCLI, Machineatron, SiteAgent, Rla],
        );
        x.perm(
            "ReplaceAllExpectedSwitches",
            vec![NicoAdminCLI, Machineatron],
        );
        x.perm(
            "DeleteAllExpectedSwitches",
            vec![NicoAdminCLI, Machineatron],
        );
        x.perm(
            "GetAllExpectedSwitchesLinked",
            vec![NicoAdminCLI, Machineatron, SiteAgent, Rla],
        );
        x.perm(
            "AddExpectedRack",
            vec![NicoAdminCLI, Machineatron, SiteAgent],
        );
        x.perm(
            "DeleteExpectedRack",
            vec![NicoAdminCLI, Machineatron, SiteAgent],
        );
        x.perm(
            "UpdateExpectedRack",
            vec![NicoAdminCLI, Machineatron, SiteAgent],
        );
        x.perm(
            "GetExpectedRack",
            vec![NicoAdminCLI, Machineatron, SiteAgent, Rla],
        );
        x.perm(
            "GetAllExpectedRacks",
            vec![NicoAdminCLI, Machineatron, SiteAgent, Rla],
        );
        x.perm("ReplaceAllExpectedRacks", vec![NicoAdminCLI, Machineatron]);
        x.perm("DeleteAllExpectedRacks", vec![NicoAdminCLI, Machineatron]);
        x.perm(
            "FindSwitchStateHistories",
            vec![NicoAdminCLI, Machineatron, Rla],
        );
        x.perm("FindRackIds", vec![NicoAdminCLI, SiteAgent, Rla]);
        x.perm("FindRacksByIds", vec![NicoAdminCLI, SiteAgent, Rla]);
        x.perm("GetRack", vec![NicoAdminCLI, Rla]);
        x.perm("DeleteRack", vec![NicoAdminCLI, Rla]);
        x.perm("GetRackProfile", vec![NicoAdminCLI]);
        x.perm("RackManagerCall", vec![NicoAdminCLI]);
        x.perm("ScoutStream", vec![Scout]);
        x.perm("ScoutStreamShowConnections", vec![NicoAdminCLI]);
        x.perm("ScoutStreamDisconnect", vec![NicoAdminCLI]);
        x.perm("ScoutStreamPing", vec![NicoAdminCLI]);
        x.perm("MlxAdminProfileSync", vec![NicoAdminCLI]);
        x.perm("MlxAdminProfileShow", vec![NicoAdminCLI]);
        x.perm("MlxAdminProfileCompare", vec![NicoAdminCLI]);
        x.perm("MlxAdminProfileList", vec![NicoAdminCLI]);
        x.perm("MlxAdminLockdownLock", vec![NicoAdminCLI]);
        x.perm("MlxAdminLockdownUnlock", vec![NicoAdminCLI]);
        x.perm("MlxAdminLockdownStatus", vec![NicoAdminCLI]);
        x.perm("MlxAdminShowDevice", vec![NicoAdminCLI]);
        x.perm("MlxAdminShowMachine", vec![NicoAdminCLI]);
        x.perm("MlxAdminRegistryList", vec![NicoAdminCLI]);
        x.perm("MlxAdminRegistryShow", vec![NicoAdminCLI]);
        x.perm("MlxAdminConfigQuery", vec![NicoAdminCLI]);
        x.perm("MlxAdminConfigSet", vec![NicoAdminCLI]);
        x.perm("MlxAdminConfigSync", vec![NicoAdminCLI]);
        x.perm("MlxAdminConfigCompare", vec![NicoAdminCLI]);
        x.perm("FindNVLinkPartitionIds", vec![NicoAdminCLI, SiteAgent]);
        x.perm("FindNVLinkPartitionsByIds", vec![NicoAdminCLI, SiteAgent]);
        x.perm("NVLinkPartitionsForTenant", vec![NicoAdminCLI, SiteAgent]);
        x.perm(
            "FindNVLinkLogicalPartitionIds",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm(
            "FindNVLinkLogicalPartitionsByIds",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm(
            "CreateNVLinkLogicalPartition",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm(
            "UpdateNVLinkLogicalPartition",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm(
            "DeleteNVLinkLogicalPartition",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm(
            "NVLinkLogicalPartitionsForTenant",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm(
            "GetMachinePositionInfo",
            vec![NicoAdminCLI, SiteAgent, Rla],
        );
        x.perm("ModifyDPFState", vec![NicoAdminCLI]);
        x.perm("GetDPFState", vec![NicoAdminCLI]);
        x.perm("UpdateMachineNvLinkInfo", vec![NicoAdminCLI]);
        x.perm("CreateComputeAllocation", vec![NicoAdminCLI, SiteAgent]);
        x.perm("FindComputeAllocationIds", vec![NicoAdminCLI, SiteAgent]);
        x.perm(
            "FindComputeAllocationsByIds",
            vec![NicoAdminCLI, SiteAgent],
        );
        x.perm("UpdateComputeAllocation", vec![NicoAdminCLI, SiteAgent]);
        x.perm("DeleteComputeAllocation", vec![NicoAdminCLI, SiteAgent]);
        x.perm("ComponentPowerControl", vec![NicoAdminCLI, Rla]);
        x.perm("GetComponentInventory", vec![NicoAdminCLI, Rla]);
        x.perm("UpdateComponentFirmware", vec![NicoAdminCLI, Rla]);
        x.perm("GetComponentFirmwareStatus", vec![NicoAdminCLI, Rla]);
        x.perm("ListComponentFirmwareVersions", vec![NicoAdminCLI, Rla]);
        x
    }
    fn perm(&mut self, msg: &str, principals: Vec<RulePrincipal>) {
        self.perms
            .insert(msg.to_string(), RuleInfo::new(principals));
    }

    pub fn allowed_from_static(msg: &str, user_principals: &[crate::auth::Principal]) -> bool {
        INTERNAL_RBAC_RULES.allowed(msg, user_principals)
    }

    pub fn allowed(&self, msg: &str, user_principals: &[crate::auth::Principal]) -> bool {
        if let Some(perm_info) = self.perms.get(msg) {
            if user_principals.is_empty() {
                // No proper cert presented, but we will allow stuff that allows just Anonymous
                return perm_info.principals.as_slice() == [Principal::Anonymous];
            }
            user_principals.iter().any(|user_principal| {
                perm_info
                    .principals
                    .iter()
                    .any(|perm_principal| user_principal.is_proper_subset_of(perm_principal))
            })
        } else {
            false
        }
    }
}

impl Default for InternalRBACRules {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
struct RuleInfo {
    principals: Vec<crate::auth::Principal>,
}

impl RuleInfo {
    pub fn new(principals: Vec<RulePrincipal>) -> Self {
        Self {
            principals: principals
                .iter()
                .map(|x| match *x {
                    RulePrincipal::NicoAdminCLI => Principal::ExternalUser(ExternalUserInfo::new(
                        None,
                        "Invalid".to_string(),
                        None,
                    )),
                    RulePrincipal::Machineatron => {
                        Principal::SpiffeServiceIdentifier("machine-a-tron".to_string())
                    }
                    RulePrincipal::SiteAgent => {
                        Principal::SpiffeServiceIdentifier("elektra-site-agent".to_string())
                    }
                    RulePrincipal::Agent => Principal::SpiffeMachineIdentifier("".to_string()),
                    RulePrincipal::Scout => Principal::SpiffeMachineIdentifier("".to_string()),
                    RulePrincipal::Dns => {
                        Principal::SpiffeServiceIdentifier("nico-dns".to_string())
                    }
                    RulePrincipal::Dhcp => {
                        Principal::SpiffeServiceIdentifier("nico-dhcp".to_string())
                    }
                    RulePrincipal::Ssh => {
                        Principal::SpiffeServiceIdentifier("nico-ssh-console".to_string())
                    }
                    RulePrincipal::SshRs => {
                        Principal::SpiffeServiceIdentifier("nico-ssh-console-rs".to_string())
                    }
                    RulePrincipal::Pxe => {
                        Principal::SpiffeServiceIdentifier("nico-pxe".to_string())
                    }
                    RulePrincipal::Health => {
                        Principal::SpiffeServiceIdentifier("nico-hardware-health".to_string())
                    }
                    RulePrincipal::Rla => {
                        Principal::SpiffeServiceIdentifier("nico-rla".to_string())
                    }
                    RulePrincipal::MaintenanceJobs => {
                        Principal::SpiffeServiceIdentifier("nico-maintenance-jobs".to_string())
                    }
                    RulePrincipal::DsxExchangeConsumer => Principal::SpiffeServiceIdentifier(
                        "nico-dsx-exchange-consumer".to_string(),
                    ),
                    RulePrincipal::Anonymous => Principal::Anonymous,
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod rbac_rule_tests {
    use std::fs::File;
    use std::io::{BufRead, BufReader};

    use super::*;
    use crate::auth::Principal;

    fn ensure_identical_permissions(princ_a: &Principal, princ_b: &Principal) {
        for (rule_name, rule) in &INTERNAL_RBAC_RULES.perms {
            if rule.principals.contains(princ_a) {
                assert!(
                    rule.principals.contains(princ_b),
                    "{} RBAC rule allows {} but not {}",
                    rule_name,
                    princ_a.as_identifier(),
                    princ_b.as_identifier(),
                );
            } else {
                assert!(
                    !rule.principals.contains(princ_b),
                    "{} RBAC rule rejects {} but allows {}",
                    rule_name,
                    princ_a.as_identifier(),
                    princ_b.as_identifier(),
                );
            }

            if rule.principals.contains(princ_b) {
                assert!(
                    rule.principals.contains(princ_a),
                    "{} RBAC rule allows {} but not {}",
                    rule_name,
                    princ_b.as_identifier(),
                    princ_a.as_identifier(),
                );
            } else {
                assert!(
                    !rule.principals.contains(princ_a),
                    "{} RBAC rule rejects {} but allows {}",
                    rule_name,
                    princ_b.as_identifier(),
                    princ_a.as_identifier(),
                );
            }
        }
    }

    #[test]
    fn rbac_rule_tests() -> Result<(), eyre::Report> {
        assert!(InternalRBACRules::allowed_from_static(
            "Version",
            &[Principal::TrustedCertificate]
        ));
        assert!(InternalRBACRules::allowed_from_static(
            "GetStoragePool",
            &[Principal::ExternalUser(ExternalUserInfo::new(
                None,
                "any".to_string(),
                None
            ))]
        ));
        assert!(!InternalRBACRules::allowed_from_static(
            "GetStoragePool",
            &[Principal::SpiffeMachineIdentifier("foo".to_string())]
        ));
        assert!(InternalRBACRules::allowed_from_static(
            "ReportNicoScoutError",
            &[Principal::SpiffeMachineIdentifier("foo".to_string())]
        ));
        assert!(!InternalRBACRules::allowed_from_static(
            "ReportNicoScoutError",
            &[Principal::ExternalUser(ExternalUserInfo::new(
                None,
                "any".to_string(),
                None
            ))]
        ));
        assert!(InternalRBACRules::allowed_from_static(
            "GetCloudInitInstructions",
            &[Principal::SpiffeServiceIdentifier(
                "nico-pxe".to_string()
            )]
        ));
        assert!(!InternalRBACRules::allowed_from_static(
            "GetCloudInitInstructions",
            &[Principal::SpiffeServiceIdentifier(
                "nico-dns".to_string()
            )]
        ));
        assert!(!InternalRBACRules::allowed_from_static(
            "GetCloudInitInstructions",
            &[Principal::ExternalUser(ExternalUserInfo::new(
                None,
                "any".to_string(),
                None
            ))]
        ));
        assert!(InternalRBACRules::allowed_from_static(
            "CreateVpc",
            &[Principal::SpiffeServiceIdentifier(
                "machine-a-tron".to_string()
            )]
        ));
        assert!(!InternalRBACRules::allowed_from_static(
            "CreateVpc",
            &[Principal::SpiffeServiceIdentifier(
                "nico-dns".to_string()
            )]
        ));

        assert!(InternalRBACRules::allowed_from_static(
            "CreateTenantKeyset",
            &[Principal::SpiffeServiceIdentifier(
                "elektra-site-agent".to_string()
            )]
        ));
        assert!(InternalRBACRules::allowed_from_static(
            "FindNetworkSegmentsByIds",
            &[
                Principal::SpiffeServiceIdentifier("machine-a-tron".to_string()),
                Principal::TrustedCertificate
            ]
        ));

        assert!(InternalRBACRules::allowed_from_static(
            "DiscoverMachine",
            &[]
        ));

        assert!(InternalRBACRules::allowed_from_static(
            "TrimTable",
            &[Principal::SpiffeServiceIdentifier(
                "nico-maintenance-jobs".to_string()
            )]
        ));

        for method in [
            "GetAllExpectedSwitches",
            "GetAllExpectedSwitchesLinked",
            "GetAllExpectedPowerShelves",
            "GetAllExpectedPowerShelvesLinked",
        ] {
            assert!(
                InternalRBACRules::allowed_from_static(
                    method,
                    &[Principal::SpiffeServiceIdentifier(
                        "elektra-site-agent".to_string()
                    )]
                ),
                "{method} should allow SiteAgent"
            );
        }

        assert!(InternalRBACRules::allowed_from_static(
            "SetMaintenance",
            &[Principal::SpiffeServiceIdentifier(
                "nico-rla".to_string()
            )]
        ));
        assert!(InternalRBACRules::allowed_from_static(
            "InsertMachineHealthReport",
            &[Principal::SpiffeServiceIdentifier(
                "nico-rla".to_string()
            )]
        ));
        assert!(InternalRBACRules::allowed_from_static(
            "RemoveMachineHealthReport",
            &[Principal::SpiffeServiceIdentifier(
                "nico-rla".to_string()
            )]
        ));
        assert!(InternalRBACRules::allowed_from_static(
            "MachineSetAutoUpdate",
            &[Principal::SpiffeServiceIdentifier(
                "nico-rla".to_string()
            )]
        ));

        // Ensure Ssh and SshRs both have identical permissions. (ssh-console-rs is a rust rewrite
        // of ssh-console, and to keep things straightforward, it has its own set of DNS names,
        // SPIFFE identifiers, etc. We don't want to play any tricks by reusing principals here, so
        // we gotta list both, until we've fully migrated to ssh-console-rs.)
        ensure_identical_permissions(
            &Principal::SpiffeServiceIdentifier("nico-ssh-console".to_string()),
            &Principal::SpiffeServiceIdentifier("nico-ssh-console-rs".to_string()),
        );

        Ok(())
    }

    #[test]
    fn all_requests_listed() -> Result<(), eyre::Report> {
        let mut messages = vec![];
        let proto = File::open(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../rpc/proto/nico.proto"),
        )?;
        let reader = BufReader::new(proto);
        for line in reader.lines() {
            let line = line?;
            let line = line.trim();
            if line.starts_with("rpc") {
                let mut name = line.strip_prefix("rpc").unwrap_or("why").trim().to_string();
                let offset = name.find("(").unwrap_or(name.len());
                name.replace_range(offset.., "");
                messages.push(name.trim().to_string());
            }
        }
        if messages.is_empty() {
            panic!("Parsing failed, no messages found")
        }
        let rules = InternalRBACRules::new();
        let mut missing = vec![];
        for msg in messages {
            if !rules.perms.contains_key(&msg) {
                missing.push(msg);
            }
        }
        if !missing.is_empty() {
            panic!("GRPC messages missing RBAC permissions: {missing:?}");
        }
        Ok(())
    }
}
