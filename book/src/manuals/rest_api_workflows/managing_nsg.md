# Managing Network Security Groups

## Retrieve All Network Security Groups

```bash
curl -X GET "https://api.ngc.nvidia.com/v2/org/{org}/carbide/network-security-group" \
  -H "Content-Type: application/json" -H "Accept: application/json" \
  -H "Authorization: Bearer ${TOKEN}"
```

**Example Output**

```json
[
  {
    "id": "string",
    "name": "string",
    "description": "string",
    "siteId": "60189e9c-7d12-438c-b9ca-6998d9c364b1",
    "tenantId": "f97df110-f4de-492e-8849-4a6af68026b0",
    "status": "Pending",
    "statusHistory": [
      {
        "status": "Pending",
        "message": "Request received, pending processing",
        "created": "2019-08-24T14:15:22Z",
        "updated": "2019-08-24T14:15:22Z"
      }
    ],
    "rules": [
      {
        "name": "string",
        "direction": "INGRESS",
        "sourcePortRange": "80-81",
        "destinationPortRange": "80-81",
        "protocol": "TCP",
        "action": "PERMIT",
        "priority": 0,
        "sourcePrefix": "10.5.44.0/24",
        "destinationPrefix": "10.5.44.0/24"
      }
    ],
    "labels": {},
    "created": "2019-08-24T14:15:22Z",
    "updated": "2019-08-24T14:15:22Z"
  }
]

## Create a Network Security Group that Limits Traffic

```bash
curl -X POST "https://api.ngc.nvidia.com/v2/org/{org}/carbide/network-security-group" \
  -H "Content-Type: application/json" -H "Accept: application/json" \
  -H "Authorization: Bearer ${TOKEN}" \
  -d '{
    "name": "string",
    "description": "string",
    "siteId": "60189e9c-7d12-438c-b9ca-6998d9c364b1",
    "rules": [
        {
          "name": "string",
          "direction": "INGRESS",
          "sourcePortRange": "80-81",
          "destinationPortRange": "80-81",
          "protocol": "TCP",
          "action": "PERMIT",
          "priority": 0,
          "sourcePrefix": "10.5.44.0/24",
          "destinationPrefix": "10.5.44.0/24"
        }
      ],
    "labels": {}
  }'
```
**Example Output**

```json
{
  "id": "string",
  "name": "string",
  "description": "string",
  "siteId": "60189e9c-7d12-438c-b9ca-6998d9c364b1",
  "tenantId": "f97df110-f4de-492e-8849-4a6af68026b0",
  "status": "Pending",
  "statusHistory": [
    {
      "status": "Pending",
      "message": "Request received, pending processing",
      "created": "2019-08-24T14:15:22Z",
      "updated": "2019-08-24T14:15:22Z"
    }
  ],
  "rules": [
    {
      "name": "string",
      "direction": "INGRESS",
      "sourcePortRange": "80-81",
      "destinationPortRange": "80-81",
      "protocol": "TCP",
      "action": "PERMIT",
      "priority": 0,
      "sourcePrefix": "10.5.44.0/24",
      "destinationPrefix": "10.5.44.0/24"
    }
  ],
  "labels": {},
  "created": "2019-08-24T14:15:22Z",
  "updated": "2019-08-24T14:15:22Z"
}
```

## Create a Network Security Group that Permits All Traffic

```bash
curl -X POST "https://api.ngc.nvidia.com/v2/org/{org}/carbide/network-security-group" \
  -H "Content-Type: application/json" -H "Accept: application/json" \
  -H "Authorization: Bearer ${TOKEN}" \
  -d '{
    "name": "string",
    "description": "string",
    "siteId": "60189e9c-7d12-438c-b9ca-6998d9c364b1",
    "rules": [
      {
        "name": "allow-all-ingress",
        "direction": "INGRESS",
        "sourcePortRange": "0-65535",
        "destinationPortRange": "0-65535",
        "protocol": "ANY",
        "action": "PERMIT",
        "priority": 0,
        "sourcePrefix": "0.0.0.0/0",
        "destinationPrefix": "0.0.0.0/0"
      },
      {
        "name": "allow-all-egress",
        "direction": "EGRESS",
        "sourcePortRange": "0-65535",
        "destinationPortRange": "0-65535",
        "protocol": "ANY",
        "action": "PERMIT",
        "priority": 0,
        "sourcePrefix": "0.0.0.0/0",
        "destinationPrefix": "0.0.0.0/0"
      }
    ],
    "labels": {
      "property1": "default",
      "property2": "global-allow"
    }
    }'
```

**Example Output**

```json
[
  {
    "id": "ea9c9eac-0e3d-4c85-b0e0-1e174b214c8f",
    "name": "allow-all",
    "description": "Allow all L4 traffic in all directions",
    "siteId": "60189e9c-7d12-438c-b9ca-6998d9c364b1",
    "tenantId": "f97df110-f4de-492e-8849-4a6af68026b0",
    "status": "Pending",
    "statusHistory": [
      {
        "status": "Pending",
        "message": "Request received, pending processing",
        "created": "2025-05-22T12:00:00Z",
        "updated": "2025-05-22T12:00:00Z"
      }
    ],
    "rules": [
      {
        "name": "allow-all-ingress",
        "direction": "INGRESS",
        "sourcePortRange": null,
        "destinationPortRange": null,
        "protocol": "ANY",
        "action": "PERMIT",
        "priority": 0,
        "sourcePrefix": "0.0.0.0/0",
        "destinationPrefix": "0.0.0.0/0"
      },
      {
        "name": "allow-all-egress",
        "direction": "EGRESS",
        "sourcePortRange": null,
        "destinationPortRange": null,
        "protocol": "ANY",
        "action": "PERMIT",
        "priority": 0,
        "sourcePrefix": "0.0.0.0/0",
        "destinationPrefix": "0.0.0.0/0"
      }
    ],
    "labels": {
      "property1": "default",
      "property2": "global-allow"
    },
    "created": "2025-05-22T12:00:00Z",
    "updated": "2025-05-22T12:00:00Z"
  }
]
```

## Modify the Rules for a Network Security Group

```bash
curl -X PATCH "https://api.ngc.nvidia.com/v2/org/{org}/carbide/network-security-group/{nsgId}" \
  -H "Content-Type: application/json" -H "Accept: application/json" \
  -H "Authorization: Bearer ${TOKEN}" \
  -d '{
    "rules": [
      {
        "name": "allow-all-ingress",
        "direction": "INGRESS",
        "sourcePortRange": "0-65535",
        "destinationPortRange": "0-65535",
        "protocol": "ANY",
        "action": "PERMIT",
        "priority": 0,
        "sourcePrefix": "192.168.1.0/24",    // UPDATED SOURCE
        "destinationPrefix": "0.0.0.0/0"
      }
    ]
  }'
```

**Example Output**

```json
{
  "id": "string",
  "name": "string",
  "description": "string",
  "siteId": "60189e9c-7d12-438c-b9ca-6998d9c364b1",
  "tenantId": "f97df110-f4de-492e-8849-4a6af68026b0",
  "status": "Pending",
  "statusHistory": [
    {
      "status": "Pending",
      "message": "Request received, pending processing",
      "created": "2019-08-24T14:15:22Z",
      "updated": "2019-08-24T14:15:22Z"
    }
  ],
  "rules": [
    {
      "name": "allow-all-ingress",
      "direction": "INGRESS",
      "sourcePortRange": "0-65535",
      "destinationPortRange": "0-65535",
      "protocol": "ANY",
      "action": "PERMIT",
      "priority": 0,
      "sourcePrefix": "192.168.1.0/24",
      "destinationPrefix": "0.0.0.0/0"
    },
    {
      "name": "allow-all-egress",
      "direction": "EGRESS",
      "sourcePortRange": "0-65535",
      "destinationPortRange": "0-65535",
      "protocol": "ANY",
      "action": "PERMIT",
      "priority": 0,
      "sourcePrefix": "0.0.0.0/0",
      "destinationPrefix": "0.0.0.0/0"
    }
  ],
  "labels": {
    "property1": "default",
    "property2": "global-allow"
  },
  "created": "2019-08-24T14:15:22Z",
  "updated": "2019-08-24T14:15:22Z"
}
```