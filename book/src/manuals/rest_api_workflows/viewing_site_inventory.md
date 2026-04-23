
# Viewing Site Inventory

## View Your Provider ID

```bash
curl -X GET "https://api.ngc.nvidia.com/v2/org/{provider-org-name}/carbide/infrastructure-provider/current" \
  -H "Content-Type: application/json" -H "Accept: application/json" \
  -H "Authorization: Bearer ${TOKEN}"
```

**Example Output**

```json
{
  "id": "16060041-f146-43fe-8c82-be48460b5583",
  "org": "provider-org-name",
  "orgDisplayName": "Demo Provider",
  "deprecations": [
    {
      "attribute": "name",
      "replacedby": null,
      "effective": "2023-09-05T00:00:00Z",
      "notice": "\"'name' has been deprecated. Please take action immediately\""
    },
    {
      "endpoint": "POST /org/:orgName/carbide/infrastructure-provider",
      "replacedby": null,
      "effective": "2023-09-05T00:00:00Z",
      "notice": "\"'POST /org/:orgName/carbide/infrastructure-provider' has been deprecated. Please take action immediately\""
    },
    {
      "endpoint": "PATCH /org/:orgName/carbide/infrastructure-provider/current",
      "replacedby": null,
      "effective": "2023-09-05T00:00:00Z",
      "notice": "\"'PATCH /org/:orgName/carbide/infrastructure-provider/current' has been deprecated. Please take action immediately\""
    }
  ],
  "created": "2022-11-21T19:15:39.257475Z",
  "updated": "2023-08-16T10:16:50.829953Z"
}
```

## View Your Sites

Use the value of `id` from the output of the preceding example as the value for the infrastructureProviderId URL parameter:

```bash
curl -X GET "https://api.ngc.nvidia.com/v2/org/{provider-org-name}/carbide/site?infrastructureProviderId=16060041-f146-43fe-8c82-be48460b5583" \
  -H "Content-Type: application/json" -H "Accept: application/json" \
  -H "Authorization: Bearer ${TOKEN}"
```

The site ID in the response is a required input for many configuration requests.

**Example Output**

```json

[
  {
    "id": "bd4692bd-da95-410e-911a-d492fe2d35f8",
    "name": "demo-site",
    "description": "nico site",
    "org": "wdksahew1rqv",
    "infrastructureProviderId": "16060041-f146-43fe-8c82-be48460b5583",
    "siteControllerVersion": null,
    "siteAgentVersion": null,
    "registrationToken": null,
    "registrationTokenExpiration": "2023-05-03T22:18:45.23286Z",
    "serialConsoleHostname": "example.console.com",
    "isSerialConsoleEnabled": true,
    "serialConsoleIdleTimeout": null,
    "serialConsoleMaxSessionLength": null,
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
        "created": "2023-05-02T22:20:04.627469Z",
        "updated": "2023-05-02T22:20:04.627469Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-05-02T22:18:45.262934Z",
        "updated": "2023-05-02T22:18:45.262934Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-05-02T22:14:56.626151Z",
        "updated": "2023-05-02T22:14:56.626151Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-05-02T22:09:53.578675Z",
        "updated": "2023-05-02T22:09:53.578675Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-05-02T22:07:17.408311Z",
        "updated": "2023-05-02T22:07:17.408311Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-05-02T22:00:23.941866Z",
        "updated": "2023-05-02T22:00:23.941866Z"
      },
      {
        "status": "Registered",
        "message": "Site has been successfully paired",
        "created": "2023-04-18T18:14:42.89657Z",
        "updated": "2023-04-18T18:14:42.89657Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-04-18T18:11:30.468363Z",
        "updated": "2023-04-18T18:11:30.468363Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-04-18T17:55:03.750783Z",
        "updated": "2023-04-18T17:55:03.750783Z"
      },
      {
        "status": "Registered",
        "message": "Site has been successfully paired",
        "created": "2023-04-17T22:07:20.436115Z",
        "updated": "2023-04-17T22:07:20.436115Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-04-17T22:06:53.437981Z",
        "updated": "2023-04-17T22:06:53.437981Z"
      },
      {
        "status": "Registered",
        "message": "Site has been successfully paired",
        "created": "2023-04-17T17:53:51.561312Z",
        "updated": "2023-04-17T17:53:51.561312Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-04-17T17:47:58.254818Z",
        "updated": "2023-04-17T17:47:58.254818Z"
      },
      {
        "status": "Registered",
        "message": "Site has been successfully paired",
        "created": "2023-04-12T00:18:59.121209Z",
        "updated": "2023-04-12T00:18:59.121209Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-04-12T00:16:42.843838Z",
        "updated": "2023-04-12T00:16:42.843838Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-04-12T00:10:42.487854Z",
        "updated": "2023-04-12T00:10:42.487854Z"
      },
      {
        "status": "Registered",
        "message": "Site has been successfully paired",
        "created": "2023-04-11T21:44:23.119458Z",
        "updated": "2023-04-11T21:44:23.119458Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-04-11T21:42:15.163993Z",
        "updated": "2023-04-11T21:42:15.163993Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-04-11T21:36:21.00716Z",
        "updated": "2023-04-11T21:36:21.00716Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-04-11T20:32:31.337992Z",
        "updated": "2023-04-11T20:32:31.337992Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-04-10T17:24:47.639071Z",
        "updated": "2023-04-10T17:24:47.639071Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-04-07T20:56:30.373496Z",
        "updated": "2023-04-07T20:56:30.373496Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-04-07T20:43:29.831621Z",
        "updated": "2023-04-07T20:43:29.831621Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-04-07T20:03:18.534612Z",
        "updated": "2023-04-07T20:03:18.534612Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-04-07T19:45:14.541934Z",
        "updated": "2023-04-07T19:45:14.541934Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-04-07T19:40:57.13975Z",
        "updated": "2023-04-07T19:40:57.13975Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-04-07T18:44:04.297108Z",
        "updated": "2023-04-07T18:44:04.297108Z"
      },
      {
        "status": "Registered",
        "message": "Site has been successfully paired",
        "created": "2023-03-28T02:19:18.971117Z",
        "updated": "2023-03-28T02:19:18.971117Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-03-28T02:18:03.709425Z",
        "updated": "2023-03-28T02:18:03.709425Z"
      },
      {
        "status": "Registered",
        "message": "Site has been successfully paired",
        "created": "2023-03-28T01:58:18.656516Z",
        "updated": "2023-03-28T01:58:18.656516Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-03-28T01:49:43.748087Z",
        "updated": "2023-03-28T01:49:43.748087Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-03-28T01:44:38.119087Z",
        "updated": "2023-03-28T01:44:38.119087Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-03-28T01:40:55.472008Z",
        "updated": "2023-03-28T01:40:55.472008Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-03-28T01:39:06.569385Z",
        "updated": "2023-03-28T01:39:06.569385Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-03-28T01:31:25.517541Z",
        "updated": "2023-03-28T01:31:25.517541Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-03-28T01:26:48.335284Z",
        "updated": "2023-03-28T01:26:48.335284Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-03-28T01:06:39.979791Z",
        "updated": "2023-03-28T01:06:39.979791Z"
      },
      {
        "status": "Registered",
        "message": "Site has been successfully paired",
        "created": "2023-03-25T02:53:55.166162Z",
        "updated": "2023-03-25T02:53:55.166162Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-03-25T01:12:37.874225Z",
        "updated": "2023-03-25T01:12:37.874225Z"
      },
      {
        "status": "Registered",
        "message": "Site has been successfully paired",
        "created": "2023-01-24T22:50:54.549322Z",
        "updated": "2023-01-24T22:50:54.549322Z"
      },
      {
        "status": "Pending",
        "message": "registration token renewed, pending pairing",
        "created": "2023-01-24T22:37:52.614496Z",
        "updated": "2023-01-24T22:37:52.614496Z"
      },
      {
        "status": "Pending",
        "message": "received site creation request, pending pairing",
        "created": "2023-01-20T21:36:41.339466Z",
        "updated": "2023-01-20T21:36:41.339466Z"
      }
    ],
    "created": "2023-01-20T21:36:41.339466Z",
    "updated": "2023-07-06T16:19:20.104353Z"
  }
]
```

## View Your Machines

Use the `id` value from the output of the preceding examples as the values for the `infrastructureProviderId` and `siteId` URL parameters.

The following sample command uses URL parameters to filter for machines that are in a `Ready` state and are not assigned an instance type.

```bash
curl -X GET "https://api.ngc.nvidia.com/v2/org/{provider-org-name}/carbide/machine?siteId=157627d6-d742-440b-ac04-77a618d94459&infrastructureProviderId=16060041-f146-43fe-8c82-be48460b5583&hasInstanceType=false&status=Ready" \
  -H "Content-Type: application/json" -H "Accept: application/json" \
  -H "Authorization: Bearer ${TOKEN}"
```

**Example Output**

```json
[
  {
    "id": "fm100hthvos96dbmmai84gsok0dn967v9fap8ublgp34kaknd9tq7pddim0",
    "infrastructureProviderId": "16060041-f146-43fe-8c82-be48460b5583",
    "siteId": "bd4692bd-da95-410e-911a-d492fe2d35f8",
    "instanceTypeId": null,
    "controllerMachineId": "fm100hthvos96dbmmai84gsok0dn967v9fap8ublgp34kaknd9tq7pddim0",
    "controllerMachineType": "PowerEdge R750",
    "hostname": "sad-uniform",
    "machineCapabilities": [
      {
        "type": "CPU",
        "name": "Intel(R) Xeon(R) Gold 6354 CPU @ 3.00GHz",
        "cores": 18,
        "threads": 36,
        "count": 2
      },
      {
        "type": "Storage",
        "name": "Dell Ent NVMe CM6 RI 1.92TB",
        "count": 3
      },
      {
        "type": "Network",
        "name": "MT42822 BlueField-2 integrated ConnectX-6 Dx network controller",
        "count": 2
      },
      {
        "type": "Network",
        "name": "NetXtreme BCM5720 2-port Gigabit Ethernet PCIe (PowerEdge Rx5xx LOM Board)",
        "count": 2
      },
      {
        "type": "Network",
        "name": "BCM57414 NetXtreme-E 10Gb/25Gb RDMA Ethernet Controller",
        "count": 2
      }
    ],
    "machineInterfaces": [
      {
        "id": "6dd628a3-cc5b-4b12-8a5b-acad0160c7c3",
        "machineId": "fm100hthvos96dbmmai84gsok0dn967v9fap8ublgp34kaknd9tq7pddim0",
        "controllerInterfaceId": "029a9fdf-c232-4e8d-9327-4c271ad7ea01",
        "controllerSegmentId": "82e53d43-72ee-468f-bf19-bdebf3877d62",
        "subnetId": null,
        "hostname": "sad-uniform",
        "isPrimary": true,
        "macAddress": "B8:3F:D2:90:97:04",
        "ipAddresses": [
          "10.180.90.10/32"
        ],
        "created": "2023-05-04T14:22:17.121178Z",
        "updated": "2023-07-06T16:16:22.777176Z"
      }
    ],
    "status": "Ready",
    "statusHistory": [],
    "created": "2023-04-13T19:37:17.954195Z",
    "updated": "2023-07-06T16:16:22.75737Z"
  },
  {
    "id": "fm100httuapjc6t4o629o5d3uu5616gimvn0smunp199mmmp1f2134nt92g",
    "infrastructureProviderId": "16060041-f146-43fe-8c82-be48460b5583",
    "siteId": "bd4692bd-da95-410e-911a-d492fe2d35f8",
    "instanceTypeId": null,
    "controllerMachineId": "fm100httuapjc6t4o629o5d3uu5616gimvn0smunp199mmmp1f2134nt92g",
    "controllerMachineType": "PowerEdge R750",
    "hostname": "social-whiskey",
    "machineCapabilities": [
      {
        "type": "Network",
        "name": "MT42822 BlueField-2 integrated ConnectX-6 Dx network controller",
        "count": 2
      },
      {
        "type": "Network",
        "name": "NetXtreme BCM5720 2-port Gigabit Ethernet PCIe (PowerEdge Rx5xx LOM Board)",
        "count": 2
      },
      {
        "type": "Storage",
        "name": "Dell Ent NVMe v2 AGN RI U.2 1.92TB",
        "count": 2
      },
      {
        "type": "CPU",
        "name": "Intel(R) Xeon(R) Gold 6354 CPU @ 3.00GHz",
        "cores": 18,
        "threads": 36,
        "count": 2
      }
    ],
    "machineInterfaces": [
      {
        "id": "41847749-03af-4c8c-94b6-6722e32a603e",
        "machineId": "fm100httuapjc6t4o629o5d3uu5616gimvn0smunp199mmmp1f2134nt92g",
        "controllerInterfaceId": "8b7ea73b-76d1-46be-8194-4c183c816a8f",
        "controllerSegmentId": "82e53d43-72ee-468f-bf19-bdebf3877d62",
        "subnetId": null,
        "hostname": "social-whiskey",
        "isPrimary": true,
        "macAddress": "B8:3F:D2:90:99:B4",
        "ipAddresses": [
          "10.180.90.15/32"
        ],
        "created": "2023-05-04T14:58:20.087651Z",
        "updated": "2023-07-06T16:16:22.861989Z"
      }
    ],
    "status": "Ready",
    "statusHistory": [],
    "created": "2023-04-25T14:19:20.895831Z",
    "updated": "2023-07-06T16:16:22.82906Z"
  }
]
```

## View Existing IP Blocks

Use the value of `id` from the output of the preceding example as the value for the `infrastructureProviderId` and `siteId` URL parameters:

```bash
curl -X GET "https://api.ngc.nvidia.com/v2/org/{provider-org-name}/carbide/ipblock?infrastructureProviderId=16060041-f146-43fe-8c82-be48460b5583&siteId=157627d6-d742-440b-ac04-77a618d94459" \
  -H "Content-Type: application/json" -H "Accept: application/json" \
  -H "Authorization: Bearer ${TOKEN}"

```

**Example Output**

```json

[
  {
    "id": "06c3d1a5-ed84-489a-96cc-4d644dc57ebb",
    "name": "test-megan",
    "description": null,
    "siteId": "157627d6-d742-440b-ac04-77a618d94459",
    "infrastructureProviderId": "16060041-f146-43fe-8c82-be48460b5583",
    "tenantId": null,
    "routingType": "Public",
    "prefix": "100.100.0.0",
    "prefixLength": 19,
    "protocolVersion": "IPv4",
    "status": "Ready",
    "statusHistory": [
      {
        "status": "Ready",
        "message": "IP Block is ready for use",
        "created": "2023-10-18T19:44:38.921567Z",
        "updated": "2023-10-18T19:44:38.921567Z"
      }
    ],
    "deprecations": [
      {
        "attribute": "blockSize",
        "replacedby": "prefixLength",
        "effective": "2023-04-15T00:00:00Z",
        "notice": "\"'blockSize' has been deprecated in favor of 'prefixLength'. Please take action immediately\""
      }
    ],
    "created": "2023-10-18T19:44:38.921567Z",
    "updated": "2023-10-18T19:44:38.921567Z"
  },
  {
    "id": "b88be53c-ba35-4b55-844b-fa581921f6f3",
    "name": "demo-ipv4-block",
    "description": "Demonstration IPv4 block",
    "siteId": "157627d6-d742-440b-ac04-77a618d94459",
    "infrastructureProviderId": "16060041-f146-43fe-8c82-be48460b5583",
    "tenantId": null,
    "routingType": "Public",
    "prefix": "192.168.20.0",
    "prefixLength": 24,
    "protocolVersion": "IPv4",
    "status": "Ready",
    "statusHistory": [
      {
        "status": "Ready",
        "message": "IP Block is ready for use",
        "created": "2023-10-16T17:20:21.214662Z",
        "updated": "2023-10-16T17:20:21.214662Z"
      }
    ],
    "deprecations": [
      {
        "attribute": "blockSize",
        "replacedby": "prefixLength",
        "effective": "2023-04-15T00:00:00Z",
        "notice": "\"'blockSize' has been deprecated in favor of 'prefixLength'. Please take action immediately\""
      }
    ],
    "created": "2023-10-16T17:20:21.214662Z",
    "updated": "2023-10-16T17:20:21.214662Z"
  },
  {
    "id": "a65e537e-ba93-4d69-ac5c-b7d177344890",
    "name": "demo-ipv4-network-gold",
    "description": "Demo IPv4 - Gold",
    "siteId": "157627d6-d742-440b-ac04-77a618d94459",
    "infrastructureProviderId": "16060041-f146-43fe-8c82-be48460b5583",
    "tenantId": null,
    "routingType": "Public",
    "prefix": "192.168.4.128",
    "prefixLength": 26,
    "protocolVersion": "IPv4",
    "status": "Ready",
    "statusHistory": [
      {
        "status": "Ready",
        "message": "IP Block is ready for use",
        "created": "2023-04-25T13:49:15.699358Z",
        "updated": "2023-04-25T13:49:15.699358Z"
      }
    ],
    "deprecations": [
      {
        "attribute": "blockSize",
        "replacedby": "prefixLength",
        "effective": "2023-04-15T00:00:00Z",
        "notice": "\"'blockSize' has been deprecated in favor of 'prefixLength'. Please take action immediately\""
      }
    ],
    "created": "2023-04-25T13:49:15.699358Z",
    "updated": "2023-04-25T13:49:15.699358Z"
  }
]
```

