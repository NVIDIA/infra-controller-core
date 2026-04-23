# Managing Virtual Private Clouds

## Create a Virtual Private Cloud (VPC)

1. Create the VPC and specify a name.

   ```bash
   curl -X POST "https://api.ngc.nvidia.com/v2/org/{tenant-org-name}/carbide/vpc" \
   -H "Content-Type: application/json" -H "Accept: application/json" \
   -H "Authorization: Bearer ${TOKEN}" \
   -d '{
      "name": "demo-vpc",
      "siteId": "bd4692bd-da95-410e-911a-d492fe2d35f8",
      "description": "Demo tenant VPC",
      "networkVirtualizationType": "FNN"
      }'
   ```

   **Example Output**

   ```json
   {
      "id": "0b1c53a0-a27e-4714-98d7-0cd3bc579db2",
      "name": "demo-vpc",
      "description": "Demo tenant VPC",
      "org": "tenant-org-name",
      "infrastructureProviderId": "16060041-f146-43fe-8c82-be48460b5583",
      "tenantId": "aaf3cb83-8785-4265-a3bd-61e828f87db8",
      "siteId": "bd4692bd-da95-410e-911a-d492fe2d35f8",
      "controllerVpcId": null,
      "networkVirtualizationType": "FNN",
      "status": "Pending",
      "statusHistory": [
         {
         "status": "Pending",
         "message": "received vpc creation request, pending pairing",
         "created": "2023-07-06T16:20:43.335989Z",
         "updated": "2023-07-06T16:20:43.335989Z"
         }
      ],
      "created": "2023-07-06T16:20:43.335989Z",
      "updated": "2023-07-06T16:20:43.335989Z"
   }
   ```

2. (Optional) Poll the VPC endpoint to confirm the status changes to `Ready`:

   ```bash
   curl GET "https://api.ngc.nvidia.com/v2/org/{tenant-org-name}/carbide/vpc/0b1c53a0-a27e-4714-98d7-0cd3bc579db2" \
   -H "Accept: application/json" -H "Authorization: Bearer ${TOKEN}"
   ```

   **Example Output**

   ```json
   {
      "id": "0b1c53a0-a27e-4714-98d7-0cd3bc579db2",
      "name": "demo-vpc",
      "description": "Demo tenant VPC",
      "org": "tenant-org-name",
      "infrastructureProviderId": "16060041-f146-43fe-8c82-be48460b5583",
      "tenantId": "aaf3cb83-8785-4265-a3bd-61e828f87db8",
      "siteId": "bd4692bd-da95-410e-911a-d492fe2d35f8",
      "controllerVpcId": "b1e32ce0-49d4-48c7-bdc9-ecc1623c02f4",
      "networkVirtualizationType": "FNN",
      "status": "Ready",
      "statusHistory": [
         {
            "status": "Ready",
            "message": "VPC successfully provisioned on Site",
            "created": "2023-07-06T16:20:44.490895Z",
            "updated": "2023-07-06T16:20:44.490895Z"
         },
         {
            "status": "Provisioning",
            "message": "initiated VPC provisioning via Site Agent",
            "created": "2023-07-06T16:20:43.63647Z",
            "updated": "2023-07-06T16:20:43.63647Z"
         },
         {
            "status": "Pending",
            "message": "received vpc creation request, pending pairing",
            "created": "2023-07-06T16:20:43.335989Z",
            "updated": "2023-07-06T16:20:43.335989Z"
         }
      ],
      "created": "2023-07-06T16:20:43.335989Z",
      "updated": "2023-07-06T16:20:44.497876Z"
   }
   ```

## Add an Instance to a VPC

### Prerequisites

- You have an operating system image ID so that the compute instances can boot the operating system.
- You have at least one IP block allocated to you so that you can add a subnet or VPC prefix with the IP block address space.
- You have at least one compute instance type allocated to you so that you can add compute instances to the VPC.

### Procedure

1. Add one or more compute instances.

   **Example Input for Instance with Single Interface**

   ```bash
   curl -X POST "https://api.ngc.nvidia.com/v2/org/{tenant-org-name}/carbide/instance" \
      -H "Content-Type: application/json" -H "Accept: application/json" \
      -H "Authorization: Bearer ${TOKEN}" \
      -d '{
         "name": "demo-compute-instance-0",
         "instanceTypeId": "9c4aaa6a-3934-4274-b0a9-5143b253039e",
         "vpcId": "0b1c53a0-a27e-4714-98d7-0cd3bc579db2",
         "tenantId": "aaf3cb83-8785-4265-a3bd-61e828f87db8",
         "operatingSystemId": "0865029e-3979-432d-985e-2de396ecce32",
         "userData": null,
         "interfaces": [
            {
               "subnetId": "5e1f6c51-a532-437b-b7a5-7dfac214de08"
            }
         ]
         }'
   ```

   **Example Output**

   ```json
   {
      "id": "a4a1895b-d696-4d90-a670-357c2f3485b6",
      "name": "demo-compute-instance-0",
      "controllerInstanceId": "",
      "allocationId": "9b06c02f-f46d-4dfc-9033-71f42e72cc7d",
      "tenantId": "aaf3cb83-8785-4265-a3bd-61e828f87db8",
      "infrastructureProviderId": "16060041-f146-43fe-8c82-be48460b5583",
      "siteId": "bd4692bd-da95-410e-911a-d492fe2d35f8",
      "instanceTypeId": "9c4aaa6a-3934-4274-b0a9-5143b253039e",
      "vpcId": "0b1c53a0-a27e-4714-98d7-0cd3bc579db2",
      "machineId": "fm100hthvos96dbmmai84gsok0dn967v9fap8ublgp34kaknd9tq7pddim0",
      "operatingSystemId": "0865029e-3979-432d-985e-2de396ecce32",
      "ipxeScript": "#!ipxe\nkernel https://github.com/netbootxyz/ubuntu-squash/releases/download/20.04.6-a1b16d57/vmlinuz ip=dhcp fb=false interface=ens5f0 url=https://releases.ubuntu.com/20.04.6/ubuntu-20.04.6-live-server-amd64.iso autoinstall ds=nocloud-net;s=${cloudinit-url} initrd=initrd.magic\ninitrd https://github.com/netbootxyz/ubuntu-squash/releases/download/20.04.6-a1b16d57/initrd\nboot\n",
      "userData": "#cloud-config\nusers:\n  - default\n  - name: demo-user\n    gecos: Demo User\n    sudo: ALL=(ALL) NOPASSWD:ALL\n    groups: root\n    lock_passwd: true\n    ssh_authorized_keys:\n      - ssh-ed25519 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAcV/3oxRllEji0wl9F6icRk+Kme0H2MMAPFizKB5yv8 demo@example.com\n\nautoinstall:\n  version: 1\n\n  identity:\n    hostname: demo-host\n    password: $6$jCfWFbdxh1lK09sY$pxFnrW/yXewYFmgoaywu3WKhdPQg0e8DR8jvedAV.udXM0.i5M6wr4Up2S7ZCN9kNDmg.s7fmrOaXE6nEyzPb/ # Welcome123\n    username: ubuntu\n\n  ntp:\n    enabled: true\n    ntp_client: chrony  # Uses cloud-init default chrony configuration\n    servers:\n      - 129.6.15.32\n\n  keyboard:\n    layout: us\n    toggle: null\n    variant: \"\"\n  locale: en_US\n  network:\n    version: 2\n    ethernets:\n      ens5f0:\n        critical: true\n        dhcp-identifier: mac\n        dhcp4: true\n        nameservers:\n          addresses: [8.8.8.8]\n  ssh:\n    allow-pw: true\n    authorized-keys: []\n    install-server: true\n\n  disk_setup:\n    ephemeral0:\n      table_type: \"mbr\"\n      layout: true\n    /dev/nvme0n1:\n      table_type: \"mbr\"\n      layout:\n        - 33\n        - [33, 82]\n        - 33\n      overwrite: True\n",
      "serialConsoleUrl": null,
      "status": "Pending",
      "interfaces": [
         {
            "id": "752bf819-a740-4e19-9f0e-b99ee9162392",
            "instanceId": "a4a1895b-d696-4d90-a670-357c2f3485b6",
            "subnetId": "5e1f6c51-a532-437b-b7a5-7dfac214de08",
            "isPhysical": true,
            "macAddress": null,
            "ipAddresses": null,
            "status": "Pending",
            "created": "2023-07-06T17:04:00.338968Z",
            "updated": "2023-07-06T17:04:00.338968Z"
         }
      ],
      "statusHistory": [
         {
            "status": "Pending",
            "message": "received instance creation request, pending",
            "created": "2023-07-06T17:04:00.338968Z",
            "updated": "2023-07-06T17:04:00.338968Z"
         }
      ],
      "deprecations": [
         {
            "attribute": "sshUrl",
            "replacedby": "serialConsoleUrl",
            "effective": "2023-04-25T00:00:00Z",
            "notice": "\"'sshUrl' has been deprecated in favor of 'serialConsoleUrl'. Please take action immediately\""
         },
         {
            "attribute": "instanceSubnets",
            "replacedby": "interfaces",
            "effective": "2023-05-10T00:00:00Z",
            "notice": "\"'instanceSubnets' has been deprecated in favor of 'interfaces'. Please take action immediately\""
         }
      ],
      "created": "2023-07-06T17:04:00.338968Z",
      "updated": "2023-07-06T17:04:00.338968Z"
   }
   ```
   
   **Example Input for Instance with Multiple Interfaces**

   ```bash
   curl -X POST "https://api.ngc.nvidia.com/v2/org/{tenant-org-name}/carbide/instance" \
      -H "Content-Type: application/json" -H "Accept: application/json" \
      -H "Authorization: Bearer ${TOKEN}" \
      -d '{
            "name": "demo-compute-instance-multiple-interfaces",
            "instanceTypeId": "364dd639-5122-420c-a663-fa56e290e187",
            "vpcId": "0b1c53a0-a27e-4714-98d7-0cd3bc579db2",
            "tenantId": "aaf3cb83-8785-4265-a3bd-61e828f87db8",
            "operatingSystemId": "0865029e-3979-432d-985e-2de396ecce32",
            "userData": null,
            "interfaces": [
               {
                  "vpcPrefixId": "8c7422d7-abf5-41ae-8b6d-9d62442a8b31",
                  "isPhysical": true,
                  "device": "MT43244 BlueField-3 integrated ConnectX-7 network controller",
                  "deviceInstance": 0
               },
               {
                  "vpcPrefixId": "8988dbd3-f038-4338-b961-8e5cbf89a77e",
                  "isPhysical": true,
                  "device": "MT43244 BlueField-3 integrated ConnectX-7 network controller",
                  "deviceInstance": 1
               }
            ]
            }'
   ```

   **Example Output**

   ```json
   {
      "id": "9db49890-d015-464d-95d9-26e6714dbf8a",
      "name": "demo-compute-instance-1",
      "controllerInstanceId": "",
      "allocationId": "9b06c02f-f46d-4dfc-9033-71f42e72cc7d",
      "tenantId": "aaf3cb83-8785-4265-a3bd-61e828f87db8",
      "infrastructureProviderId": "16060041-f146-43fe-8c82-be48460b5583",
      "siteId": "bd4692bd-da95-410e-911a-d492fe2d35f8",
      "instanceTypeId": "9c4aaa6a-3934-4274-b0a9-5143b253039e",
      "vpcId": "0b1c53a0-a27e-4714-98d7-0cd3bc579db2",
      "machineId": "fm100httuapjc6t4o629o5d3uu5616gimvn0smunp199mmmp1f2134nt92g",
      "operatingSystemId": "0865029e-3979-432d-985e-2de396ecce32",
      "ipxeScript": "#!ipxe\nkernel https://github.com/netbootxyz/ubuntu-squash/releases/download/20.04.6-a1b16d57/vmlinuz ip=dhcp fb=false interface=ens5f0 url=https://releases.ubuntu.com/20.04.6/ubuntu-20.04.6-live-server-amd64.iso autoinstall ds=nocloud-net;s=${cloudinit-url} initrd=initrd.magic\ninitrd https://github.com/netbootxyz/ubuntu-squash/releases/download/20.04.6-a1b16d57/initrd\nboot\n",
      "userData": "#cloud-config\nusers:\n  - default\n  - name: demo-user\n    gecos: Demo User\n    sudo: ALL=(ALL) NOPASSWD:ALL\n    groups: root\n    lock_passwd: true\n    ssh_authorized_keys:\n      - ssh-ed25519 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAcV/3oxRllEji0wl9F6icRk+Kme0H2MMAPFizKB5yv8 demo@example.com\n\nautoinstall:\n  version: 1\n\n  identity:\n    hostname: demo-host\n    password: $6$jCfWFbdxh1lK09sY$pxFnrW/yXewYFmgoaywu3WKhdPQg0e8DR8jvedAV.udXM0.i5M6wr4Up2S7ZCN9kNDmg.s7fmrOaXE6nEyzPb/ # Welcome123\n    username: ubuntu\n\n  ntp:\n    enabled: true\n    ntp_client: chrony  # Uses cloud-init default chrony configuration\n    servers:\n      - 129.6.15.32\n\n  keyboard:\n    layout: us\n    toggle: null\n    variant: \"\"\n  locale: en_US\n  network:\n    version: 2\n    ethernets:\n      ens5f0:\n        critical: true\n        dhcp-identifier: mac\n        dhcp4: true\n        nameservers:\n          addresses: [8.8.8.8]\n  ssh:\n    allow-pw: true\n    authorized-keys: []\n    install-server: true\n\n  disk_setup:\n    ephemeral0:\n      table_type: \"mbr\"\n      layout: true\n    /dev/nvme0n1:\n      table_type: \"mbr\"\n      layout:\n        - 33\n        - [33, 82]\n        - 33\n      overwrite: True\n",
      "serialConsoleUrl": null,
      "status": "Pending",
      "interfaces": [
            {
            "id": "277fec57-abbc-43e1-8d81-1ae7b960b22f",
            "instanceId": "9db49890-d015-464d-95d9-26e6714dbf8a",
            "vpcPrefixId": "8c7422d7-abf5-41ae-8b6d-9d62442a8b31",
            "device": "MT43244 BlueField-3 integrated ConnectX-7 network controller",
            "deviceInstance": 0,
            "isPhysical": true,
            "macAddress": null,
            "ipAddresses": null,
            "status": "Pending",
            "created": "2023-07-06T17:07:36.268362Z",
            "updated": "2023-07-06T17:07:36.268362Z"
            },
            {
            "id": "62220366-4453-4f25-ae7c-d0109285c06f",
            "instanceId": "9db49890-d015-464d-95d9-26e6714dbf8a",
            "vpcPrefixId": "8c7422d7-abf5-41ae-8b6d-9d62442a8b31",
            "device": "MT43244 BlueField-3 integrated ConnectX-7 network controller",
            "deviceInstance": 1,
            "isPhysical": true,
            "macAddress": null,
            "ipAddresses": null,
            "status": "Pending",
            "created": "2023-07-06T17:07:36.268362Z",
            "updated": "2023-07-06T17:07:36.268362Z"
            }
      ],
      "statusHistory": [
         {
            "status": "Pending",
            "message": "received instance creation request, pending",
            "created": "2023-07-06T17:07:36.268362Z",
            "updated": "2023-07-06T17:07:36.268362Z"
         }
      ],
      "deprecations": [
         {
            "attribute": "sshUrl",
            "replacedby": "serialConsoleUrl",
            "effective": "2023-04-25T00:00:00Z",
            "notice": "\"'sshUrl' has been deprecated in favor of 'serialConsoleUrl'. Please take action immediately\""
         },
         {
            "attribute": "instanceSubnets",
            "replacedby": "interfaces",
            "effective": "2023-05-10T00:00:00Z",
            "notice": "\"'instanceSubnets' has been deprecated in favor of 'interfaces'. Please take action immediately\""
         }
      ],
      "created": "2023-07-06T17:07:36.268362Z",
      "updated": "2023-07-06T17:07:36.268362Z"
   }
   ```

2. (Optional) Poll the instance to confirm the status changes to `Ready`:

   ```bash
   curl GET "https://api.ngc.nvidia.com/v2/org/{tenant-org-name}/carbide/instance/a4a1895b-d696-4d90-a670-357c2f3485b6" \
      -H "Accept: application/json" -H "Authorization: Bearer ${TOKEN}"
   ```

   **Example Output**

   ```json
   {
      "id": "a4a1895b-d696-4d90-a670-357c2f3485b6",
      "name": "demo-compute-instance-0",
      "controllerInstanceId": "53ecddf2-fbaf-432f-9231-dc2f6bb8cf28",
      "allocationId": "9b06c02f-f46d-4dfc-9033-71f42e72cc7d",
      "tenantId": "aaf3cb83-8785-4265-a3bd-61e828f87db8",
      "infrastructureProviderId": "16060041-f146-43fe-8c82-be48460b5583",
      "siteId": "bd4692bd-da95-410e-911a-d492fe2d35f8",
      "instanceTypeId": "9c4aaa6a-3934-4274-b0a9-5143b253039e",
      "vpcId": "0b1c53a0-a27e-4714-98d7-0cd3bc579db2",
      "machineId": "fm100hthvos96dbmmai84gsok0dn967v9fap8ublgp34kaknd9tq7pddim0",
      "operatingSystemId": "0865029e-3979-432d-985e-2de396ecce32",
      "ipxeScript": "#!ipxe\nkernel https://github.com/netbootxyz/ubuntu-squash/releases/download/20.04.6-a1b16d57/vmlinuz ip=dhcp fb=false interface=ens5f0 url=https://releases.ubuntu.com/20.04.6/ubuntu-20.04.6-live-server-amd64.iso autoinstall ds=nocloud-net;s=${cloudinit-url} initrd=initrd.magic\ninitrd https://github.com/netbootxyz/ubuntu-squash/releases/download/20.04.6-a1b16d57/initrd\nboot\n",
      "userData": "#cloud-config\nusers:\n  - default\n  - name: demo-user\n    gecos: Demo User\n    sudo: ALL=(ALL) NOPASSWD:ALL\n    groups: root\n    lock_passwd: true\n    ssh_authorized_keys:\n      - ssh-ed25519 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAcV/3oxRllEji0wl9F6icRk+Kme0H2MMAPFizKB5yv8 demo@example.com\n\nautoinstall:\n  version: 1\n\n  identity:\n    hostname: demo-host\n    password: $6$jCfWFbdxh1lK09sY$pxFnrW/yXewYFmgoaywu3WKhdPQg0e8DR8jvedAV.udXM0.i5M6wr4Up2S7ZCN9kNDmg.s7fmrOaXE6nEyzPb/ # Welcome123\n    username: ubuntu\n\n  ntp:\n    enabled: true\n    ntp_client: chrony  # Uses cloud-init default chrony configuration\n    servers:\n      - 129.6.15.32\n\n  keyboard:\n    layout: us\n    toggle: null\n    variant: \"\"\n  locale: en_US\n  network:\n    version: 2\n    ethernets:\n      ens5f0:\n        critical: true\n        dhcp-identifier: mac\n        dhcp4: true\n        nameservers:\n          addresses: [8.8.8.8]\n  ssh:\n    allow-pw: true\n    authorized-keys: []\n    install-server: true\n\n  disk_setup:\n    ephemeral0:\n      table_type: \"mbr\"\n      layout: true\n    /dev/nvme0n1:\n      table_type: \"mbr\"\n      layout:\n        - 33\n        - [33, 82]\n        - 33\n      overwrite: True\n",
      "serialConsoleUrl": "ssh://53ecddf2-fbaf-432f-9231-dc2f6bb8cf28@demo-site.example.com",
      "status": "Ready",
      "interfaces": [
         {
            "id": "752bf819-a740-4e19-9f0e-b99ee9162392",
            "instanceId": "a4a1895b-d696-4d90-a670-357c2f3485b6",
            "subnetId": "5e1f6c51-a532-437b-b7a5-7dfac214de08",
            "isPhysical": true,
            "macAddress": null,
            "ipAddresses": [
            "192.166.128.2"
            ],
            "status": "Ready",
            "created": "2023-07-06T17:04:00.338968Z",
            "updated": "2023-07-06T17:07:23.628998Z"
         }
      ],
      "statusHistory": [
         {
            "status": "BootCompleted",
            "message": "Instance is ready for use",
            "created": "2023-07-06T17:07:23.618478Z",
            "updated": "2023-07-06T17:07:23.618478Z"
         },
         {
            "status": "Provisioning",
            "message": "Instance provisioning was successfully initiated on Site",
            "created": "2023-07-06T17:04:03.277049Z",
            "updated": "2023-07-06T17:04:03.277049Z"
         },
         {
            "status": "Provisioning",
            "message": "Provisioning request was sent to the Site",
            "created": "2023-07-06T17:04:02.269136Z",
            "updated": "2023-07-06T17:04:02.269136Z"
         },
         {
            "status": "Pending",
            "message": "received instance creation request, pending",
            "created": "2023-07-06T17:04:00.338968Z",
            "updated": "2023-07-06T17:04:00.338968Z"
         }
      ],
      "deprecations": [
         {
            "attribute": "sshUrl",
            "replacedby": "serialConsoleUrl",
            "effective": "2023-04-25T00:00:00Z",
            "notice": "\"'sshUrl' has been deprecated in favor of 'serialConsoleUrl'. Please take action immediately\""
         },
         {
            "attribute": "instanceSubnets",
            "replacedby": "interfaces",
            "effective": "2023-05-10T00:00:00Z",
            "notice": "\"'instanceSubnets' has been deprecated in favor of 'interfaces'. Please take action immediately\""
         }
      ],
      "created": "2023-07-06T17:04:00.338968Z",
      "updated": "2023-07-06T17:07:23.608281Z"
   }
   ```