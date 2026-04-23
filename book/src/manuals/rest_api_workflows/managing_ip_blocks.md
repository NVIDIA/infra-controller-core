# Managing IP Blocks

## Add an IP Block

```bash
curl -X POST "/v2/org/{provider-org-name}/carbide/ipblock" \
  -H "Content-Type: application/json" -H "Accept: application/json" \
  -H "Authorization: Bearer ${TOKEN}" \
  -d '{
        "name": "demo-ipv4-block",
        "description": "Demo IPv4 block",
        "prefixLength": 24,
        "prefix": "192.166.128.0",
        "protocolVersion": "IPv4",
        "routingType": "DatacenterOnly",
        "siteId": "bd4692bd-da95-410e-911a-d492fe2d35f8"
      }'
```

**Example Output**

```json

{
  "id": "ff920227-e2a1-43aa-99bd-7e39653e4f9f",
  "name": "demo-ipv4-block",
  "description": "Demo IPv4 block",
  "siteId": "bd4692bd-da95-410e-911a-d492fe2d35f8",
  "infrastructureProviderId": "16060041-f146-43fe-8c82-be48460b5583",
  "tenantId": null,
  "routingType": "DatacenterOnly",
  "prefix": "192.166.128.0",
  "prefixLength": 24,
  "protocolVersion": "IPv4",
  "status": "Ready",
  "statusHistory": [
    {
      "status": "Ready",
      "message": "IP Block is ready for use",
      "created": "2023-07-06T16:17:46.911267Z",
      "updated": "2023-07-06T16:17:46.911267Z"
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
  "created": "2023-07-06T16:17:46.911267Z",
  "updated": "2023-07-06T16:17:46.911267Z"
}
```

## Allocate an IP Block

```bash
curl -X POST "/v2/org/{provider-org-name}/carbide/allocation" \
  -H "Content-Type: application/json" -H "Accept: application/json" \
  -H "Authorization: Bearer ${TOKEN}" \
  -d '{
        "name": "demo-ipv4-allocation",
        "description": "Demo IPv4 allocation",
        "siteId": "bd4692bd-da95-410e-911a-d492fe2d35f8",
        "tenantId": "aaf3cb83-8785-4265-a3bd-61e828f87db8",
        "allocationConstraints": [
          {
            "resourceType": "IPBlock",
            "resourceTypeId": "ff920227-e2a1-43aa-99bd-7e39653e4f9f",
            "constraintType": "Reserved",
            "constraintValue": 24
          }
        ]
      }'
Example Output

{
  "id": "98c356e0-0c96-45ef-a65d-319338190955",
  "name": "demo-ipv4-allocation",
  "description": "Demo IPv4 allocation",
  "infrastructureProviderId": "16060041-f146-43fe-8c82-be48460b5583",
  "tenantId": "aaf3cb83-8785-4265-a3bd-61e828f87db8",
  "siteId": "bd4692bd-da95-410e-911a-d492fe2d35f8",
  "status": "Registered",
  "statusHistory": [
    {
      "status": "Registered",
      "message": "received allocation creation request, registered",
      "created": "2023-07-06T16:18:02.513471Z",
      "updated": "2023-07-06T16:18:02.513471Z"
    }
  ],
  "created": "2023-07-06T16:18:02.513471Z",
  "updated": "2023-07-06T16:18:02.513471Z",
  "allocationConstraints": [
    {
      "id": "5eef34d5-8644-4a5c-9604-a4a72b42118e",
      "allocationId": "98c356e0-0c96-45ef-a65d-319338190955",
      "resourceType": "IPBlock",
      "ResourceTypeID": "ff920227-e2a1-43aa-99bd-7e39653e4f9f",
      "constraintType": "Reserved",
      "constraintValue": 24,
      "derivedResourceId": "20d7dd4f-ae43-4245-a9d9-d093296009c4",
      "created": "2023-07-06T16:18:02.513471Z",
      "updated": "2023-07-06T16:18:02.513471Z"
    }
  ]
}
```