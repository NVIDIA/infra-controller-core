# Managing Subnets and VPC Prefixes

## Prerequisites

You should have at least one IP block allocated so that you can add a subnet of the IP block address space.

## Add a Subnet

1. Add one or more subnets. The following command sample shows how to add one subnet.

   ```bash
    curl -X POST "https://api.ngc.nvidia.com/v2/org/{tenant-org-name}/carbide/subnet" \
    -H "Content-Type: application/json" -H "Accept: application/json" \
    -H "Authorization: Bearer ${TOKEN}" \
    -d '{
            "name": "demo-ipv4-subnet",
            "description": "Demo IPv4 Tenant Subnet",
            "vpcId": "0b1c53a0-a27e-4714-98d7-0cd3bc579db2",
            "ipv4BlockId": "20d7dd4f-ae43-4245-a9d9-d093296009c4",
            "prefixLength": 28
        }'
   ```

   **Example Output**

   ```json
    {
        "id": "5e1f6c51-a532-437b-b7a5-7dfac214de08",
        "name": "demo-ipv4-subnet",
        "description": "Demo IPv4 Tenant Subnet",
        "siteId": "bd4692bd-da95-410e-911a-d492fe2d35f8",
        "vpcId": "0b1c53a0-a27e-4714-98d7-0cd3bc579db2",
        "tenantId": "aaf3cb83-8785-4265-a3bd-61e828f87db8",
        "controllerNetworkSegmentId": null,
        "ipv4Prefix": "192.166.128.0",
        "ipv4BlockId": "20d7dd4f-ae43-4245-a9d9-d093296009c4",
        "ipv4Gateway": "192.166.128.1",
        "ipv6Prefix": null,
        "ipv6BlockId": null,
        "ipv6Gateway": null,
        "mtu": 9000,
        "prefixLength": 28,
        "routingType": "Public",
        "status": "Pending",
        "statusHistory": [
            {
            "status": "Pending",
            "message": "received subnet creation request, pending",
            "created": "2023-07-06T16:21:34.916407Z",
            "updated": "2023-07-06T16:21:34.916407Z"
            }
        ],
        "deprecations": [
            {
            "attribute": "ipBlockSize",
            "replacedby": "prefixLength",
            "effective": "2023-04-15T00:00:00Z",
            "notice": "\"'ipBlockSize' has been deprecated in favor of 'prefixLength'. Please take action immediately\""
            }
        ],
        "created": "2023-07-06T16:21:34.916407Z",
        "updated": "2023-07-06T16:21:34.916407Z"
    }
   ```

2. (Optional) Poll the subnet endpoint to confirm that the status changes to `Ready`:

   ```bash
   curl GET "https://api.ngc.nvidia.com/v2/org/{tenant-org-name}/carbide/subnet/5e1f6c51-a532-437b-b7a5-7dfac214de08" \
   -H "Accept: application/json" -H "Authorization: Bearer ${TOKEN}" \
   ```

   **Example Output**

   ```json
    {
        "id": "5e1f6c51-a532-437b-b7a5-7dfac214de08",
        "name": "demo-ipv4-subnet",
        "description": "Demo IPv4 Tenant Subnet",
        "siteId": "bd4692bd-da95-410e-911a-d492fe2d35f8",
        "vpcId": "0b1c53a0-a27e-4714-98d7-0cd3bc579db2",
        "tenantId": "aaf3cb83-8785-4265-a3bd-61e828f87db8",
        "controllerNetworkSegmentId": "f5634c31-4cd4-453b-8ac5-e81f2a19ab05",
        "ipv4Prefix": "192.166.128.0",
        "ipv4BlockId": "20d7dd4f-ae43-4245-a9d9-d093296009c4",
        "ipv4Gateway": "192.166.128.1",
        "ipv6Prefix": null,
        "ipv6BlockId": null,
        "ipv6Gateway": null,
        "mtu": 9000,
        "prefixLength": 28,
        "status": "Ready",
        "statusHistory": [
            {
            "status": "Ready",
            "message": "Subnet is ready for use",
            "created": "2023-07-06T16:25:20.717794Z",
            "updated": "2023-07-06T16:25:20.717794Z"
            },
            {
            "status": "Provisioning",
            "message": "Subnet provisioning was successfully initiated on Site",
            "created": "2023-07-06T16:21:36.830555Z",
            "updated": "2023-07-06T16:21:36.830555Z"
            },
            {
            "status": "Provisioning",
            "message": "Provisioning request was sent to the Site",
            "created": "2023-07-06T16:21:35.538119Z",
            "updated": "2023-07-06T16:21:35.538119Z"
            },
            {
            "status": "Pending",
            "message": "received subnet creation request, pending",
            "created": "2023-07-06T16:21:34.916407Z",
            "updated": "2023-07-06T16:21:34.916407Z"
            }
        ],
        "deprecations": [
            {
            "attribute": "ipBlockSize",
            "replacedby": "prefixLength",
            "effective": "2023-04-15T00:00:00Z",
            "notice": "\"'ipBlockSize' has been deprecated in favor of 'prefixLength'. Please take action immediately\""
            }
        ],
        "created": "2023-07-06T16:21:34.916407Z",
        "updated": "2023-07-06T16:25:20.70757Z"
    }

## Add a VPC Prefix

The following command sample shows how to add one VPC prefix. You can also add multiple VPC prefixes at once.

```bash
curl -X POST "https://api.ngc.nvidia.com/v2/org/{tenant-org-name}/carbide/vpc-prefix" \
  -H "Content-Type: application/json" -H "Accept: application/json" \
  -H "Authorization: Bearer ${TOKEN}" \
  -d '{
        "name": "demo-ipv4-vpc-prefix",
        "description": "Demo IPv4 Tenant VPC Prefix",
        "vpcId": "0b1c53a0-a27e-4714-98d7-0cd3bc579db2",
        "ipBlockId": "20d7dd4f-ae43-4245-a9d9-d093296009c4",
        "prefixLength": 28
      }'
```

    Example Output

```json
{
  "id": "0c03ba01-d86b-4a57-a41e-cc359b380a6f",
  "name": "demo-ipv4-vpc-prefix",
  "description": "Demo IPv4 Tenant VPC Prefix",
  "siteId": "bd4692bd-da95-410e-911a-d492fe2d35f8",
  "vpcId": "0b1c53a0-a27e-4714-98d7-0cd3bc579db2",
  "tenantId": "aaf3cb83-8785-4265-a3bd-61e828f87db8",
  "ipBlockId": "20d7dd4f-ae43-4245-a9d9-d093296009c4",
  "prefix": "10.217.98.208/28", 
  "prefixLength": 28,
  "status": "Ready",
  "statusHistory": [
    {
      "status": "Ready",
      "message": "Received VPC prefix creation request, ready",
      "created": "2023-07-06T16:25:20.717794Z",
      "updated": "2023-07-06T16:25:20.717794Z"
    }
  ],
  "created": "2023-07-06T16:21:34.916407Z",
  "updated": "2023-07-06T16:25:20.70757Z"
}
```