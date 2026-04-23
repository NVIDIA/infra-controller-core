# Accessing the Serial Console

## Enable the Serial Console on a Compute Instance

```bash
curl -X PATCH "https://api.ngc.nvidia.com/v2/org/{provider-org-name}/carbide/site/2ae25bd4-7b07-4b39-9514-031e5c335f4f" \
  -H "Content-Type: application/json" -H "Accept: application/json" \
  -H "Authorization: Bearer ${TOKEN}" \
  -d '{
        "isSerialConsoleEnabled": true,
        "serialConsoleHostname": "10.217.126.53",
        "serialConsoleIdleTimeout": 7200,
        "serialConsoleMaxSessionLength": 86400
      }'
```

**Example Output**

```json
{
  "id": "2ae25bd4-7b07-4b39-9514-031e5c335f4f",
  "name": "demo-site-a",
  "description": "Demo Site A",
  "org": "provider-org-name",
  "infrastructureProviderId": "16060041-f146-43fe-8c82-be48460b5583",
  "siteControllerVersion": null,
  "siteAgentVersion": null,
  "registrationToken": null,
  "registrationTokenExpiration": "2023-11-01T01:47:36.000397Z",
  "serialConsoleHostname": "192.168.126.53",
  "isSerialConsoleEnabled": true,
  "serialConsoleIdleTimeout": 7200,
  "serialConsoleMaxSessionLength": 86400,
  "isOnline": true,
  "deprecations": [
    {
      "attribute": "sshHostname",
      "replacedby": "serialConsoleHostname",
      "effective": "2023-04-28T00:00:00Z",
      "notice": "\"'sshHostname' has been deprecated in favor of 'serialConsoleHostname'. Please take action immediately\""
    }
  ],
  "status": "Registered",
  "statusHistory": [
    {
      "status": "Registered",
      "message": "Site has been successfully paired",
      "created": "2023-10-31T01:49:19.604387Z",
      "updated": "2023-10-31T01:49:19.604387Z"
    }
  ],
  "created": "2023-06-07T00:56:04.261575Z",
  "updated": "2023-11-01T15:02:01.914645Z"
}
```

## Add a Public Key to the Key Management System

### Prerequisites

The SSH key should be in RSA, ECDSA, or ED25519 format.

### Procedure

1. Add the SSH key group:

   ```bash
   curl -X POST "https://api.ngc.nvidia.com/v2/org/{tenant-org-name}/carbide/sshkeygroup" \
   -H "Content-Type: application/json" -H "Accept: application/json" \
   -H "Authorization: Bearer ${TOKEN}" \
   -d '{
         "name": "demo-team-0-group",
         "description": "Demo team group"
      }'
   ```

   **Example Output**

   ```json
   {
   "id": "9ffb8f90-f88f-4420-952d-e911f446d7eb",
   "name": "demo-team-0-group",
   "description": "Demo team group",
   "org": "tenant-org-name",
   "tenantId": "778086ad-35b8-46ff-a796-69b2e4c93975",
   "version": "6a5ccd83b5daf693bd14ab32a439d3181635be6f",
   "status": "Synced",
   "statusHistory": [
      {
         "status": "Syncing",
         "message": "received SSH Key Group creation request, syncing",
         "created": "2023-11-01T12:37:22.649619Z",
         "updated": "2023-11-01T12:37:22.649619Z"
      },
      {
         "status": "Synced",
         "message": "SSH Key Group has successfully been synced to all Sites",
         "created": "2023-11-01T12:37:22.649619Z",
         "updated": "2023-11-01T12:37:22.649619Z"
      }
   ],
   "sshKeys": [],
   "siteAssociations": [],
   "created": "2023-11-01T12:37:22.649619Z",
   "updated": "2023-11-01T12:37:22.693571Z"
   }
   ```

   You need the value of the `version` field to update the key group.

2. Add the public SSH key:

   ```bash
   curl -X POST "https://api.ngc.nvidia.com/v2/org/{tenant-org-name}/carbide/sshkey" \
   -H "Content-Type: application/json" -H "Accept: application/json" \
   -H "Authorization: Bearer ${TOKEN}" \
   -d '{
         "name": "customer-0",
         "description": "Demo public SSH key",
         "publicKey": "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAcV/3oxRllEji0wl9F6icRk+Kme0H2MMAPFizKB5yv8 demo-user@nvdia.com"
         }'
   ```

   **Example Output**

   ```json
   {
   "id": "b658db7e-f06c-4140-9494-48ea1f3f7769",
   "name": "customer-0",
   "org": "tenant-org-name",
   "tenantId": "778086ad-35b8-46ff-a796-69b2e4c93975",
   "fingerprint": "LfniSSO1iwx1nbAXyP6swmwmkrW3GnW1j9v+t/Ou9Vw",
   "created": "2023-11-01T12:37:30.554055Z",
   "updated": "2023-11-01T12:37:30.554055Z"
   }
   ```

3. Add the public SSH key to the key group.

   Specify the new and existing key IDs to keep in the `sshKeyIds` field.

   ```bash
   curl -X PATCH "https://api.ngc.nvidia.com/v2/org/{tenant-org-name}/carbide/sshkeygroup/9ffb8f90-f88f-4420-952d-e911f446d7eb" \
   -H "Content-Type: application/json" -H "Accept: application/json" \
   -H "Authorization: Bearer ${TOKEN}" \
   -d '{
         "version": "6a5ccd83b5daf693bd14ab32a439d3181635be6f",
         "sshKeyIds": [
           "b658db7e-f06c-4140-9494-48ea1f3f7769"
         ]
       }'
   ```

   **Example Output**

   ```json
   {
      "id": "9ffb8f90-f88f-4420-952d-e911f446d7eb",
      "name": "demo-team-0-group",
      "description": "Demo team group",
      "org": "tenant-org-name",
      "tenantId": "778086ad-35b8-46ff-a796-69b2e4c93975",
      "version": "23220d11a579258cb810942060e910fb3fac9762",
      "status": "Syncing",
      "statusHistory": [
         {
            "status": "Syncing",
            "message": "received SSH Key Group update request, syncing",
            "created": "2023-11-01T12:37:44.969029Z",
            "updated": "2023-11-01T12:37:44.969029Z"
         },
         {
            "status": "Syncing",
            "message": "received SSH Key Group creation request, syncing",
            "created": "2023-11-01T12:37:22.649619Z",
            "updated": "2023-11-01T12:37:22.649619Z"
         },
         {
            "status": "Synced",
            "message": "SSH Key Group has successfully been synced to all Sites",
            "created": "2023-11-01T12:37:22.649619Z",
            "updated": "2023-11-01T12:37:22.649619Z"
         }
      ],
      "sshKeys": [
         {
            "id": "b658db7e-f06c-4140-9494-48ea1f3f7769",
            "name": "customer-0",
            "org": "tenant-org-name",
            "tenantId": "778086ad-35b8-46ff-a796-69b2e4c93975",
            "fingerprint": "LfniSSO1iwx1nbAXyP6swmwmkrW3GnW1j9v+t/Ou9Vw",
            "created": "2023-11-01T12:37:30.554055Z",
            "updated": "2023-11-01T12:37:30.554055Z"
         }
      ],
      "siteAssociations": [],
      "created": "2023-11-01T12:37:22.649619Z",
      "updated": "2023-11-01T12:37:45.007738Z"
   }
   ```

4. Add the sites to the key group:

   Specify the new and existing site IDs to keep in the `siteIds` field. You can combine this step and the preceding step by specifying both the SSH key IDs and site IDs in the same request.

   ```bash
   curl -X PATCH "https://api.ngc.nvidia.com/v2/org/{tenant-org-name}/carbide/sshkeygroup/9ffb8f90-f88f-4420-952d-e911f446d7eb" \
   -H "Content-Type: application/json" -H "Accept: application/json" \
   -H "Authorization: Bearer ${TOKEN}" \
   -d '{
         "version": "23220d11a579258cb810942060e910fb3fac9762",
         "siteIds": [
            "157627d6-d742-440b-ac04-77a618d94459"
         ]
         }'
   ```

   **Example Output**

   ```json
   {
      "id": "9ffb8f90-f88f-4420-952d-e911f446d7eb",
      "name": "demo-team-0-group",
      "description": "Demo team group",
      "org": "tenant-org-name",
      "tenantId": "778086ad-35b8-46ff-a796-69b2e4c93975",
      "version": "7fa801416ce27ec68d9dd531eb6948a9a5f4ed87",
      "status": "Syncing",
      "statusHistory": [
         {
            "status": "Syncing",
            "message": "received SSH Key Group update request, syncing",
            "created": "2023-11-01T12:38:04.208273Z",
            "updated": "2023-11-01T12:38:04.208273Z"
         },
         {
            "status": "Syncing",
            "message": "received SSH Key Group update request, syncing",
            "created": "2023-11-01T12:37:44.969029Z",
            "updated": "2023-11-01T12:37:44.969029Z"
         },
         {
            "status": "Syncing",
            "message": "received SSH Key Group creation request, syncing",
            "created": "2023-11-01T12:37:22.649619Z",
            "updated": "2023-11-01T12:37:22.649619Z"
         },
         {
            "status": "Synced",
            "message": "SSH Key Group has successfully been synced to all Sites",
            "created": "2023-11-01T12:37:22.649619Z",
            "updated": "2023-11-01T12:37:22.649619Z"
         }
      ],
      "sshKeys": [
         {
            "id": "b658db7e-f06c-4140-9494-48ea1f3f7769",
            "name": "customer-0",
            "org": "tenant-org-name",
            "tenantId": "778086ad-35b8-46ff-a796-69b2e4c93975",
            "fingerprint": "LfniSSO1iwx1nbAXyP6swmwmkrW3GnW1j9v+t/Ou9Vw",
            "created": "2023-11-01T12:37:30.554055Z",
            "updated": "2023-11-01T12:37:30.554055Z"
         }
      ],
      "siteAssociations": [
         {
            "site": {
            "id": "157627d6-d742-440b-ac04-77a618d94459",
            "name": "demo-site-a",
            "description": "Demo Site",
            "org": "provider-org-name",
            "infrastructureProviderId": "16060041-f146-43fe-8c82-be48460b5583",
            "siteControllerVersion": null,
            "siteAgentVersion": null,
            "registrationToken": null,
            "registrationTokenExpiration": null,
            "serialConsoleHostname": "192.168.126.6",
            "isSerialConsoleEnabled": true,
            "serialConsoleIdleTimeout": 300,
            "serialConsoleMaxSessionLength": 86400,
            "isSerialConsoleSSHKeysEnabled": false,
            "isOnline": true,
            "deprecations": [
               {
                  "attribute": "sshHostname",
                  "replacedby": "serialConsoleHostname",
                  "effective": "2023-04-28T00:00:00Z",
                  "notice": "\"'sshHostname' has been deprecated in favor of 'serialConsoleHostname'. Please take action immediately\""
               }
            ],
            "status": "Registered",
            "statusHistory": [],
            "created": "2023-04-21T17:30:08.425798Z",
            "updated": "2023-11-01T12:37:53.579769Z"
            },
            "version": "3af0d2e8136292010bb23afeb8de1c009e343cb2",
            "status": "Syncing",
            "created": "2023-11-01T12:38:04.208273Z",
            "updated": "2023-11-01T12:38:04.256966Z"
         }
      ],
      "created": "2023-11-01T12:37:22.649619Z",
      "updated": "2023-11-01T12:38:04.251866Z"
   }
   ```

## Access the Serial Console

1. Get information about the compute instances in the VPC:

   ```bash
   curl -X GET "https://api.ngc.nvidia.com/v2/org/{tenant-org-name}/carbide/instance?siteId=bd4692bd-da95-410e-911a-d492fe2d35f8&vpcId=f466a2d5-5820-4824-a845-3218fdff801b" \
   -H "Content-Type: application/json" -H "Accept: application/json" \
   -H "Authorization: Bearer ${TOKEN}"
   ```

   **Example Output**

   ```json
   [
      {
         "id": "83ad26bc-4687-4427-b01c-0857599d1b17",
         "name": "demo-compute-instance-1",
         "controllerInstanceId": "c0da5caa-76d6-4a5a-abf7-992b75fec7ae",
         "allocationId": "c4d1b674-d061-4e95-946e-667149a88113",
         "tenantId": "aaf3cb83-8785-4265-a3bd-61e828f87db8",
         "infrastructureProviderId": "16060041-f146-43fe-8c82-be48460b5583",
         "siteId": "bd4692bd-da95-410e-911a-d492fe2d35f8",
         "instanceTypeId": "7aa0864b-71b2-4253-af7d-d957e288ff57",
         "vpcId": "f466a2d5-5820-4824-a845-3218fdff801b",
         "machineId": "2ff5f788-7430-424a-86f8-87d371256496",
         "operatingSystemId": "7471cecd-88f6-40f0-b1a2-d53f29573bf3",
         "ipxeScript": "#!ipxe\nkernel https://github.com/netbootxyz/ubuntu-squash/releases/download/20.04.6-a1b16d57/vmlinuz ip=dhcp fb=false interface=ens5f0 url=https://releases.ubuntu.com/20.04.6/ubuntu-20.04.6-live-server-amd64.iso autoinstall ds=nocloud-net;s=${cloudinit-url} initrd=initrd.magic\ninitrd https://github.com/netbootxyz/ubuntu-squash/releases/download/20.04.6-a1b16d57/initrd\nboot\n",
         "userdata": "#cloud-config\nusers:\n  - default\n  - name: demo-user\n    gecos: Demo User\n    sudo: ALL=(ALL) NOPASSWD:ALL\n    groups: root\n    lock_passwd: true\n    ssh_authorized_keys:\n      - ssh-ed25519 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAcV/3oxRllEji0wl9F6icRk+Kme0H2MMAPFizKB5yv8 demo@example.com\n\nautoinstall:\n  version: 1\n\n  identity:\n    hostname: demo-host\n    password: $6$jCfWFbdxh1lK09sY$pxFnrW/yXewYFmgoaywu3WKhdPQg0e8DR8jvedAV.udXM0.i5M6wr4Up2S7ZCN9kNDmg.s7fmrOaXE6nEyzPb/ # Welcome123+\n    username: ubuntu\n\n  ntp:\n    enabled: true\n    ntp_client: chrony  # Uses cloud-init default chrony configuration\n    servers:\n      - 129.6.15.32\n\n  keyboard:\n    layout: us\n    toggle: null\n    variant: \"\"\n  locale: en_US\n  network:\n    version: 2\n    ethernets:\n      ens5f0:\n        critical: true\n        dhcp-identifier: mac\n        dhcp4: true\n        nameservers:\n          addresses: [8.8.8.8]\n  ssh:\n    allow-pw: true\n    authorized-keys: []\n    install-server: true\n\n  disk_setup:\n    ephemeral0:\n      table_type: \"mbr\"\n      layout: true\n    /dev/nvme0n1:\n      table_type: \"mbr\"\n      layout:\n        - 33\n        - [33, 82]\n        - 33\n      overwrite: True\n",
         "serialConsoleUrl": "ssh://c0da5caa-76d6-4a5a-abf7-992b75fec7ae@reno.carbide.nvidia.com",
         "status": "Ready",
         "instanceSubnets": [
            {
            "id": "09142b2f-85c3-4e09-aeb2-34e7db19a36d",
            "instanceId": "83ad26bc-4687-4427-b01c-0857599d1b17",
            "subnetId": "548e925c-fb5f-449f-9ec1-2dc3d89c8e9d",
            "isPhysical": true,
            "macAddress": null,
            "ipAddresses": [
               "192.166.128.3"
            ],
            "status": "Ready",
            "created": "2023-04-26T19:37:32.445684Z",
            "updated": "2023-04-26T19:40:18.427811Z"
            }
         ],
         "interfaces": [
            {
            "id": "09142b2f-85c3-4e09-aeb2-34e7db19a36d",
            "instanceId": "83ad26bc-4687-4427-b01c-0857599d1b17",
            "subnetId": "548e925c-fb5f-449f-9ec1-2dc3d89c8e9d",
            "isPhysical": true,
            "macAddress": null,
            "ipAddresses": [
               "192.166.128.3"
            ],
            "status": "Ready",
            "created": "2023-04-26T19:37:32.445684Z",
            "updated": "2023-04-26T19:40:18.427811Z"
            }
         ],
         "statusHistory": [
            {
            "status": "BootCompleted",
            "message": "Instance is ready for use",
            "created": "2023-04-26T19:40:18.418073Z",
            "updated": "2023-04-26T19:40:18.418073Z"
            },
            {
            "status": "Provisioning",
            "message": "Instance provisioning was successfully initiated on Site",
            "created": "2023-04-26T19:37:34.405577Z",
            "updated": "2023-04-26T19:37:34.405577Z"
            },
            {
            "status": "Provisioning",
            "message": "Provisioning request was sent to the Site",
            "created": "2023-04-26T19:37:33.599048Z",
            "updated": "2023-04-26T19:37:33.599048Z"
            },
            {
            "status": "Pending",
            "message": "received instance creation request, pending",
            "created": "2023-04-26T19:37:32.445684Z",
            "updated": "2023-04-26T19:37:32.445684Z"
            }
         ],
         "deprecations": [
            {
            "notice": "\"'sshUrl' is being deprecated in favor of 'serialConsoleUrl'. Please take action prior to the effective date\"",
            "field": "sshUrl",
            "effective": "2023-04-25T00:00:00Z"
            },
            {
            "notice": "\"'instanceSubnets' is being deprecated in favor of 'interfaces'. Please take action prior to the effective date\"",
            "field": "instanceSubnets",
            "effective": "2023-05-10T00:00:00Z"
            }
         ],
         "created": "2023-04-26T19:37:32.445684Z",
         "updated": "2023-04-26T19:40:18.40905Z"
      },
      {
         "id": "01e9969c-9a84-4c2d-82fb-973dff30cfc1",
         "name": "demo-compute-instance-0",
         "controllerInstanceId": "16708e55-012c-46b6-a1c6-815d45ba45bc",
         "allocationId": "c4d1b674-d061-4e95-946e-667149a88113",
         "tenantId": "aaf3cb83-8785-4265-a3bd-61e828f87db8",
         "infrastructureProviderId": "16060041-f146-43fe-8c82-be48460b5583",
         "siteId": "bd4692bd-da95-410e-911a-d492fe2d35f8",
         "instanceTypeId": "7aa0864b-71b2-4253-af7d-d957e288ff57",
         "vpcId": "f466a2d5-5820-4824-a845-3218fdff801b",
         "machineId": "e6e1b8b2-5ae5-4093-997e-eecb8baf528a",
         "operatingSystemId": "7471cecd-88f6-40f0-b1a2-d53f29573bf3",
         "ipxeScript": "#!ipxe\nkernel https://github.com/netbootxyz/ubuntu-squash/releases/download/20.04.6-a1b16d57/vmlinuz ip=dhcp fb=false interface=ens5f0 url=https://releases.ubuntu.com/20.04.6/ubuntu-20.04.6-live-server-amd64.iso autoinstall ds=nocloud-net;s=${cloudinit-url} initrd=initrd.magic\ninitrd https://github.com/netbootxyz/ubuntu-squash/releases/download/20.04.6-a1b16d57/initrd\nboot\n",
         "userdata": "#cloud-config\nusers:\n  - default\n  - name: demo-user\n    gecos: Demo User\n    sudo: ALL=(ALL) NOPASSWD:ALL\n    groups: root\n    lock_passwd: true\n    ssh_authorized_keys:\n      - ssh-ed25519 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAcV/3oxRllEji0wl9F6icRk+Kme0H2MMAPFizKB5yv8 demo@example.com\n\nautoinstall:\n  version: 1\n\n  identity:\n    hostname: demo-host\n    password: $6$jCfWFbdxh1lK09sY$pxFnrW/yXewYFmgoaywu3WKhdPQg0e8DR8jvedAV.udXM0.i5M6wr4Up2S7ZCN9kNDmg.s7fmrOaXE6nEyzPb/ # Welcome123+\n    username: ubuntu\n\n  ntp:\n    enabled: true\n    ntp_client: chrony  # Uses cloud-init default chrony configuration\n    servers:\n      - 129.6.15.32\n\n  keyboard:\n    layout: us\n    toggle: null\n    variant: \"\"\n  locale: en_US\n  network:\n    version: 2\n    ethernets:\n      ens5f0:\n        critical: true\n        dhcp-identifier: mac\n        dhcp4: true\n        nameservers:\n          addresses: [8.8.8.8]\n  ssh:\n    allow-pw: true\n    authorized-keys: []\n    install-server: true\n\n  disk_setup:\n    ephemeral0:\n      table_type: \"mbr\"\n      layout: true\n    /dev/nvme0n1:\n      table_type: \"mbr\"\n      layout:\n        - 33\n        - [33, 82]\n        - 33\n      overwrite: True\n",
         "serialConsoleUrl": "ssh://16708e55-012c-46b6-a1c6-815d45ba45bc@reno.carbide.nvidia.com",
         "status": "Ready",
         "instanceSubnets": [
            {
            "id": "10758364-57f9-4429-84a2-5dede1b5045f",
            "instanceId": "01e9969c-9a84-4c2d-82fb-973dff30cfc1",
            "subnetId": "548e925c-fb5f-449f-9ec1-2dc3d89c8e9d",
            "isPhysical": true,
            "macAddress": null,
            "ipAddresses": [
               "192.166.128.2"
            ],
            "status": "Ready",
            "created": "2023-04-26T19:34:40.96039Z",
            "updated": "2023-04-26T19:40:18.372682Z"
            }
         ],
         "interfaces": [
            {
            "id": "10758364-57f9-4429-84a2-5dede1b5045f",
            "instanceId": "01e9969c-9a84-4c2d-82fb-973dff30cfc1",
            "subnetId": "548e925c-fb5f-449f-9ec1-2dc3d89c8e9d",
            "isPhysical": true,
            "macAddress": null,
            "ipAddresses": [
               "192.166.128.2"
            ],
            "status": "Ready",
            "created": "2023-04-26T19:34:40.96039Z",
            "updated": "2023-04-26T19:40:18.372682Z"
            }
         ],
         "statusHistory": [
            {
            "status": "BootCompleted",
            "message": "Instance is ready for use",
            "created": "2023-04-26T19:37:18.21838Z",
            "updated": "2023-04-26T19:37:18.21838Z"
            },
            {
            "status": "Provisioning",
            "message": "Instance provisioning was successfully initiated on Site",
            "created": "2023-04-26T19:34:43.651979Z",
            "updated": "2023-04-26T19:34:43.651979Z"
            },
            {
            "status": "Provisioning",
            "message": "Provisioning request was sent to the Site",
            "created": "2023-04-26T19:34:42.278639Z",
            "updated": "2023-04-26T19:34:42.278639Z"
            },
            {
            "status": "Pending",
            "message": "received instance creation request, pending",
            "created": "2023-04-26T19:34:40.96039Z",
            "updated": "2023-04-26T19:34:40.96039Z"
            }
         ],
         "deprecations": [
            {
            "notice": "\"'sshUrl' is being deprecated in favor of 'serialConsoleUrl'. Please take action prior to the effective date\"",
            "field": "sshUrl",
            "effective": "2023-04-25T00:00:00Z"
            },
            {
            "notice": "\"'instanceSubnets' is being deprecated in favor of 'interfaces'. Please take action prior to the effective date\"",
            "field": "instanceSubnets",
            "effective": "2023-05-10T00:00:00Z"
            }
         ],
         "created": "2023-04-26T19:34:40.96039Z",
         "updated": "2023-04-26T19:37:18.191004Z"
      }
   ]
   ```

2. Use a command like the following to retrieve the IP addresses from the response:

   ```bash
   jq '[ .[] | {name: .name, ipAddresses: .instanceSubnets[].ipAddresses } ]'
   ```

   **Example Output**

   ```json
   [
   {
      "name": "demo-compute-instance-1",
      "ipAddresses": [
         "192.166.128.3"
      ]
   },
   {
      "name": "demo-compute-instance-0",
      "ipAddresses": [
         "192.166.128.2"
      ]
   }
   ]
   ```

   Access the host or application that you deployed on the compute instance.

