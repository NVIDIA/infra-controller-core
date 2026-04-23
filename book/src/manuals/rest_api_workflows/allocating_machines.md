# Allocating Machines

## Prerequisites

You should have the ID of the instance type. You can get the ID by making a `GET` request to the `/v2/org/{org-name}/carbide/instance/type` endpoint and specifying the `infrastructureProviderId=<provider-id>` and `siteId=<site-id>` parameters.

## Allocate Compute Instances

```bash
curl -X POST "https://api.ngc.nvidia.com/v2/org/{provider-org-name}/carbide/allocation" \
  -H "Content-Type: application/json" -H "Accept: application/json" \
  -H "Authorization: Bearer ${TOKEN}" \
  -d '{
        "name": "demo-compute-allocation",
        "description": "Demo compute instance allocation",
        "siteId": "bd4692bd-da95-410e-911a-d492fe2d35f8",
        "tenantId": "aaf3cb83-8785-4265-a3bd-61e828f87db8",
        "allocationConstraints": [
          {
            "resourceType": "InstanceType",
            "resourceTypeId": "9c4aaa6a-3934-4274-b0a9-5143b253039e",
            "constraintType": "Reserved",
            "constraintValue": 2
          }
        ]
      }'
```

**Example Output**

```json

{
  "id": "9b06c02f-f46d-4dfc-9033-71f42e72cc7d",
  "name": "demo-compute-allocation",
  "description": "Demo compute instance allocation",
  "infrastructureProviderId": "16060041-f146-43fe-8c82-be48460b5583",
  "tenantId": "aaf3cb83-8785-4265-a3bd-61e828f87db8",
  "siteId": "bd4692bd-da95-410e-911a-d492fe2d35f8",
  "status": "Registered",
  "statusHistory": [
    {
      "status": "Registered",
      "message": "received allocation creation request, registered",
      "created": "2023-07-06T16:18:59.35228Z",
      "updated": "2023-07-06T16:18:59.35228Z"
    }
  ],
  "created": "2023-07-06T16:18:59.35228Z",
  "updated": "2023-07-06T16:18:59.35228Z",
  "allocationConstraints": [
    {
      "id": "13eab768-4e65-4582-84de-d524be8a7830",
      "allocationId": "9b06c02f-f46d-4dfc-9033-71f42e72cc7d",
      "resourceType": "InstanceType",
      "ResourceTypeID": "9c4aaa6a-3934-4274-b0a9-5143b253039e",
      "constraintType": "Reserved",
      "constraintValue": 2,
      "derivedResourceId": null,
      "created": "2023-07-06T16:18:59.35228Z",
      "updated": "2023-07-06T16:18:59.35228Z"
    }
  ]
}
```