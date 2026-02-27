# SPIFFE JWT SVIDs for Machine Identity

## Software Design Document

## Revision History

| Version | Date | Modified By | Description |
| :---: | :---: | :---- | :---- |
| 0.1 | 02/24/2026 | Binu Ramakrishnan | Initial version |
|  |  |  |  |

# **1\. Introduction**

This design document specifies how the Bare Metal Manager project will integrate the SPIFFE identity framework to issue and manage machine identities using SPIFFE Verifiable Identity Documents (SVIDs). SPIFFE provides a vendor-agnostic standard for service identity that enables cryptographically verifiable identities for workloads, removing reliance on static credentials and supporting zero-trust authentication across distributed systems.

The document outlines the architecture, data models, APIs, security considerations, and interactions between Bare Metal Manager components and SPIFFE-compliant systems.

## **1.1 Purpose**

The purpose of this document is to articulate the design of the software system, ensuring all stakeholders have a shared understanding of the solution, its components, and their interactions. It details the high-level and low-level design choices, architecture, and implementation details necessary for the development.

## **1.2 Definitions and Acronyms**

| Term/Acronym | Definition |
| :---- | :---- |
| Carbide | NVIDIA bare-metal life-cycle management system (project name: Bare metal manager) |
| SDD | Software Design Document |
| API | Application Programming Interface |
| Tenant | A Carbide client/org/account that provisions/manages BM nodes through Carbide APIs. |
| DPU | Data Processing Unit \- aka SmartNIC |
| Carbide API server | A gRPC server deployed as part of Carbide site controller |
| Vault | Secrets management system (OSS version: openbao) |
| Carbide REST server | An HTTP REST-based API server that manages/proxies multiple site controllers |
| Carbide site controller | Carbide control plane services running on a local K8S cluster |
| JWT | JSON Web Token |
| SPIFFE | [SPIFFE](https://spiffe.io/) is an industry standard that provides strongly attested, cryptographic identities to workloads across a wide variety of platforms. |
| SPIRE | A specific open source software implementation of SPIFFE standard |
| SVID | SPIFFE Verifiable Identity Document (SVID). An SVID is the document with which a workload proves its identity to a resource or caller. |
| JWT-SVID | JWT-SVID is a JWT-based SVID based on the SPIFFE specification set. |
| JWKS | A JSON Web Key ([JWK](https://datatracker.ietf.org/doc/html/rfc7517)) is a JavaScript Object Notation (JSON) data structure that represents a cryptographic key.  JSON Web Key Set (JWKS) defines a JSON data structure that represents a set of JWKs. |
| IMDS | Instance Meta-data Service |
| BM | A bare metal machine \- often referred as a machine or node in this document.  |
| Token Exchange Server | A service capable of validating security tokens provided to it and issuing new security tokens in response, which enables clients to obtain appropriate access credentials for resources in heterogeneous environments or across security domains. Defined in [RFC 8693](https://datatracker.ietf.org/doc/html/rfc8693). This document also refer this as 'token endpoints' and 'token delegation server'  |

## **1.3 Scope**

This SDD covers the design for Carbide issuing SPIFFE compliant JWTs to nodes it manages. This includes the initial configuration, run-time and operational flows.

### **1.3.1​ Assumptions, Constraints, Dependencies**

* Must implement SPIFFE SVIDs as Carbide node identity
* Must rotate and expire SVIDs  
* Must provide configurable audience in SVIDs  
* Must enable delegating node identity signing  
* Must support per-tenant key for signing JWT-SVIDs   
* Must produce tokens consumable by SPIFFE-enabled services.

# **2\. System Architecture**

## **2.1 High-Level Architecture**

From a high level, the goal for Carbide is to issue a JWT-SVID identity to the requesting nodes under Carbide’s management. A Carbide managed node will be part of a tenant (aka org), and the issued JWT-SVID embodies both tenant and machine identity that complies with the SPIFFE format.

![](carbide-spiffe-jwt-svid-flow.svg)

*Figure-1 High-level architecture and flow diagram*

1. The bare metal (BM) tenant process makes HTTP requests to the Carbide meta-data service (IMDS) over a link-local address(169.254.169.254). IMDS is running inside the DPU as part of the Carbide DPU agent.   
2. IMDS in turn makes an mTLS authenticated request to the Carbide site controller gRPC server to sign a SPIFFE compliant node identity token (JWT-SVID).  
   a. Pull keys and machine and org metadata from the database, decrypt private key and sign JWT-SVID. The token is returned to Host’s tenant process (implicit, not shown in the diagram).
3. The tenant process subsequently makes a request to a service (say OpenBao/Vault) with the JWT-SVID token passed in the authentication header.  
   a. The server-x using the prefetched public keys from Carbide will validate JWT-SVID

An additional requirement for Carbide is to delegate the issuance of a JWT-SVID to an external system. The solution is to offer a callback API for Carbide tenants to intercept the signing request, validate the Carbide node identity, and issue new tenant specific JWT-SVID token (Figure-2). The delegation model offers tenants flexibility to customize their machine SVIDs.

![](carbide-spiffe-svid-token-exchange-flow.svg)

*Figure-2 Token exchange delegation flow diagram*

## **2.2 Component Breakdown**

The system is composed of the following major components:

| Component | Description |
| :---- | :---- |
| Meta-data service (IMDS) | A service part of Carbide DPU agent running inside DPU, listening on port 80 (def) |
| Carbide API (gRPC) server | Site controller Carbide control plane API server  |
| Carbide REST | Carbide REST API server, an aggregator service that controls multiple site controllers |
| Database (Postgres) | Store Carbide node-lifecycle and accounting data  |
| Token Exchange Server | Optional \- hosted by tenants to exchange Carbide node JWT-SVIDs with tenant-customized workload JWT-SVIDs. Follows token exchange API model defined in [RFC-8693](https://datatracker.ietf.org/doc/html/rfc8693) |

# **3\. Detailed Design**

There are three different flows associated with implementing this feature:

1. *Per-tenant signing key provisioning*: Describes how a new signing key associated with a tenant is provisioned, and optionally the token delegation/exchange flows.  
2. *SPIFFE key bundle discovery*: Discuss about how the signing public keys are distributed to interested parties (verifiers)  
3. *JWT-SVID node identity request flow*: The run time flow used by tenant applications to fetch JWT-SVIDs from Carbide.

Each of these flows are discussed below.

## **3.1 Per-tenant Signing Key Provisioning**

```
CreateTenantRequest (called by admin)
              │
              ▼
┌───────────────────────────────┐
│ 1. Validate request metadata  │
└───────────────────────────────┘
              │
              ▼
┌───────────────────────────────┐
│ 2. Create tenant in DB        │
│    INSERT INTO tenants        │
└───────────────────────────────┘
              │
              ▼
┌───────────────────────────────┐
│ 3. Create SVID signing keypair│
│    in DB and encrypt it       │
│    with a master key[NEW STEP]│
└───────────────────────────────┘
              │
              ▼
┌───────────────────────────────┐
│ 4. Return CreateTenantResponse│
└───────────────────────────────┘
```
*Figure-3 Per-tenant signing key provisioning flow* 

## **3.2 Per-tenant SPIFFE Key Bundle Discovery**

[SPIFFE bundles](https://spiffe.io/docs/latest/spiffe-specs/spiffe_trust_domain_and_bundle/#4-spiffe-bundle-format) are represented as an [RFC 7517](https://tools.ietf.org/html/rfc7517) compliant JWK Set. Carbide exposes the signing public keys through Carbide-rest OIDC discovery and JWKS endpoints. Services that require JWT-SVID verification pull public keys to verify token signature. Review sequence diagrams Figure-4 and 5 for more details.

```
┌────────┐       ┌───────────────┐       ┌─────────────┐       ┌──────────┐      
│ Client │       │ Carbide-rest  │       │ Carbide API │       │ Database │      
│(e.g LL)│       │   (REST)      │       │   (gRPC)    │       │(Postgres)│      
└───┬────┘       └──────┬────────┘       └──────┬──────┘       └────┬─────┘      
    │                   │                       │                   │                    
    │ GET /v2/{org-id}/ │                       │                   │
    │ {site-id}/.well-known/                    │                   │
    │ openid-configuration│                     │                   │                    
    │──────────────────>│                       │                   │                    
    │                   │                       │                   │                    
    │                   │ gRPC: GetOpenIDConfiguration              │ 
    │                   │ (site_id. org_id)     │                   │                    
    │                   │──────────────────────>│                   │                    
    │                   │                       │                   │                    
    │                   │                       │ SELECT tenant, pubkey                  
    │                   │                       │ WHERE org_id=?    │                    
    │                   │                       │──────────────────>│                    
    │                   │                       │                   │                    
    │                   │                       │ Tenant record     │                    
    │                   │                       │ (validates org    │                    
    │                   │                       │  exists)          │                    
    │                   │                       │<──────────────────│                    
    │                   │                       │                   │                    
    │                   │                       │ ┌─────────────────────────────────┐    
    │                   │                       │ │ Build OIDC Discovery Document   │    
    │                   │                       │ └─────────────────────────────────┘    
    │                   │                       │                   │                    
    │                   │ gRPC Response:        │                   │                    
    │                   │ OidcConfigResponse    │                   │ 
    │                   │<──────────────────────│                   │                    
    │                   │                       │                   │                    
    │ 200 OK            │                       │                   │                    
    │ {                 │                       │                   │                    
    │  "issuer": "...", │                       │                   │                    
    │  "jwks_uri": ".", │                       │                   │                    
    │  ...              │                       │                   │                    
    │ }                 │                       │                   │                    
    │<──────────────────│                       │                   │                    
    │                   │                       │                   │                    
```
*Figure-4 Per-tenant OIDC discovery URL flow*

```
┌────────┐       ┌───────────────┐       ┌─────────────┐       ┌──────────┐       
│ Client │       │ Carbide-rest  │       │ Carbide API │       │ Database │       
│        │       │   (REST)      │       │   (gRPC)    │       │(Postgres)│       
└───┬────┘       └──────┬────────┘       └──────┬──────┘       └────┬─────┘       
    │                   │                       │                   │                    
    │ GET /v2/{site}/   │                       │                   │                    
    │ {org}/.well-known/│                       │                   │                    
    │ jwks.json         │                       │                   │                    
    │──────────────────►│                       │                   │                    
    │                   │                       │                   │                    
    │                   │ GetJWKS(site_id,      │                   │                    
    │                   │         org_id)       │                   │                    
    │                   │ (gRPC)                │                   │                    
    │                   │──────────────────────►│                   │                    
    │                   │                       │                   │                    
    │                   │                       │ SELECT * FROM     │                    
    │                   │                       │ tenants WHERE     │                    
    │                   │                       │ org_id=? AND      │                    
    │                   │                       │ site_id=?         │                    
    │                   │                       │──────────────────►│                    
    │                   │                       │                   │                    
    │                   │                       │ Tenant record     │                    
    │                   │                       │◄──────────────────│                    
    │                   │                       │                   │                    
    │                   │                       │                   │                    
    │                   │                       │ ┌─────────────────────────────────┐    
    │                   │                       │ │ Convert key info to JWKS:       │    
    │                   │                       │ │ - Generate kid from org+version │    
    │                   │                       │ │ - Set other key fields          │    
    │                   │                       │ └─────────────────────────────────┘    
    │                   │                       │                   │                    
    │                   │ gRPC JWKS Response    │                   │  
    │                   │ {keys: [...]}         │                   │                    
    │                   │◄──────────────────────│                   │                    
    │                   │                       │                   │                    
    │ 200 OK            │                       │                   │                    
    │ Content-Type:     │                       │                   │                    
    │ application/json  │                       │                   │                    
    │                   │                       │                   │                    
    │ {"keys":[{        │                       │                   │                    
    │  "kty":"EC",      │                       │                   │                    
    │  "alg":"ES256",   │                       │                   │                   
    │  "use":"sig",     │                       │                   │                    
    │  "kid":"...",     │                       │                   │                    
    │  "crv":"P-256",   │                       │                   │                    
    │  "x":"...",       │                       │                   │                    
    │  "y":"..."        │                       │                   │                    
    │ }]}               │                       │                   │                    
    │◄──────────────────│                       │                   │                    
    │                   │                       │                   │                   
```
*Figure-5 Per-tenant SPIFFE OIDC JWKS flow*

## **3.3 JWT-SVID Node Identity Request Flow**

This is the core part of this SDD – issuing JWT-SVID based node identity tokens to the tenant node. The tenant can then use this token to authenticate with other services based on the standard SPIFFE scheme.  
​​
```
[ Tenant Workload ]
      │
      │ GET http://169.254.169.254:80/v1/meta-data/identity?aud=openbao
      ▼
[ DPU Carbide IMDS ]
      │
      │ SignMachineIdentity(..)
      ▼
[ Carbide API Server ]
      │
      │ - Validates the request (and attest)
      ▼
JWT-SVID issued to workload/tenant
```
*Figure-6 Node Identity request flow (direct, no callback)*

```
[ Tenant Workload ]
      │
      │ GET http://169.254.169.254:80/v1/meta-data/identity?aud=openbao
      ▼
[ DPU Carbide IMDS ]
      │
      │ SignMachineIdentity(..)
      ▼
[ Carbide API Server ]
      │
      │ Attest requesting machine and issue a scoped machine JWT-SVID
      ▼
[ Tenant Token Exchange Server Callback API ]
      │
      │ - Validates Carbide JWT-SVID signature using SPIFFE bundle
      │ - Verifies iss, audience, TTL and additional lookups/checks
      ▼
Carbide Tenant issue JWT-SVID to tenant workload, routed back through Carbide
```
*Figure-7 Node Identity request flow with token exchange delegation*

## **3.4 Data Model and Storage**

### **3.4.1 Database Design**
A new table will be created to store tenant signing key pairs. The private key will be encrypted with a master key stored in Vault.

| tenant\_identity\_keys |  |  |
| :---- | :---- | :---- |
| `VARCHAR(255)` | `tenant_organization_id` | PK |
| `TEXT` | `encrypted_signing_key` | Encrypted private key |
| `VARCHAR(255)` | `signing_key_public` | Public key |
| `VARCHAR(255)` | `svid_sign_callback_url` | Callback URL |
| `VARCHAR(255)` | `key_id` | Key identifier (e.g. for JWKS kid) |
| `VARCHAR(255)` | `algorithm` | Signing algorithm |
| `VARCHAR(255)` | `master_key_id` | To identify master key used for encrypting signing key |
| `BOOLEAN` | `is_active` |  |

### **3.4.2 Configuration**

The JWT spec and vault related configs are passed to the Carbide API server during startup through `site_config.toml` config file. 

```bash
# In site config file (e.g., site_config.toml)
[machine-identity]
spiffe_domain = "spiffe://carbide-domain" // prefix appear in jwt-svid 'sub'
site_id = "carbide site UUID"
iss = "https://carbide.nvidia.com" // issuer uri, appended by org_id and site_id in runtime
algorithm = "ES256"
default_aud = "carbide"
token_ttl_seconds = 300
token_delegation_http_proxy = "https://carbide-ext.com" # mitigate SSRF
```

### **3.4.3 JWT-SVID Token Format**

The subject format complies with the SPIFFE ID specification.

**Carbide JWT-SPIFFE (passed to Tenant Layer):**

```json
{
  "sub": "spiffe://{carbide-domain}/{org-id}/machine-121",
  "iss": "https://{carbide-rest}/v2/org/{org-id}/carbide/site/{site-id}",
  "aud": [
    "tenant-layer-exchange-token-service"
  ],
  "exp": 1678886400,
  "iat": 1678882800,
  "nbf": 1678882800,
  "request-meta-data" : {
    "aud": [
      "openbao-service"
    ]
  }
}
```

The Carbide issues two types of JWT-SVIDs. Though they both are similar in structure and signed by the same key, the purpose and some fields are different. 

1. If the token delegation callback is registered, Carbide issues a JWT-SVID node identity with `aud` set to `subject_token_audience`, validity/ttl limited to 120 seconds and pass additional request parameters using `request-meta-data`. This token (see example token above) is then passed to the `token_endpoint` URI. 
2. If no callback is registered, Carbide issues a JWT-SVID directly to the tenant process in the Carbide managed node. Here the `aud` is set to what is passed as parameters in the IMDS call and ttl is set to 10 minutes (configurable).

**SPIFFE JWT-SVID Issued by Token Exchange Server:**

This is a sample JWT-SVID issued by the tenant's token endpoint.

```json
{
  "sub": "spiffe://{tenant-domain}/machine/{instance-uuid}",
  "iss": "https://{tenant-domain}",
  "aud": [
    "openbao-service"
  ],
  "exp": 1678886400,
  "iat": 1678882800,
}
```

## **3.5 Component Details**

### **3.5.1 External/User-facing APIs**

#### **3.5.1.1 Metadata Identity API**

Both json and plaintext responses are supported depending on the Accept header. Defaults to json. The audience query parameter must be url encoded. Multiple audiences are allowed but discouraged by the SPIFFE spec, so we also support multiple audiences in this API. 

Request:

```bash
GET http://169.254.169.254:80/v1/meta-data/identity?aud=urlencode(spiffe://your.target.service.com)&aud=urlencode(spiffe://extra.audience.com)
Accept: application/json (or omitted)
Metadata: true
```

Response:

```bash
200 OK
Content-Type: application/json
Content-Length: ...
{
  "access_token":"...",
  "issued_token_type":
      "urn:ietf:params:oauth:token-type:jwt",
  "token_type":"Bearer",
  "expires_in": ...
 }
```

Request:

```bash
GET http://169.254.169.254:80/v1/meta-data/identity?aud=urlencode(spiffe://your.target.service.com)&aud=urlencode(spiffe://extra.audience.com)
Accept: text/plain
Metadata: true
```

Response:

```bash
200 OK
Content-Type: text/plain
Content-Length: ...
eyJhbGciOiJSUzI1NiIs...
```

#### **3.5.1.2 Carbide Token Exchange Server Registration APIs**

These APIs allow Carbide tenants to register their Token Exchange server endpoints. Delegation gives tenants control over token structure, lifecycle, and management—for example, when tenants have richer context than Carbide about the requesting workload (e.g., VM identity, application role) and need to issue tenant-specific JWT-SVIDs.

```bash
PUT /token-delegation
GET /token-delegation
DELETE /token-delegation
```

Tenant Layer calls this Carbide API to register the token exchange callback API.

Request:

```bash
PUT https://{carbide-rest}/v2/org/{org-id}/carbide/site/{site-id}/token-delegation
{
  "token_endpoint": "https://auth.acme.com/oauth2/token",
  "auth_method": "client_secret_basic",
  "client_id": "abc123",
  "client_secret": "super-secret"
  “subject_token_audience”: “value”, // to include in carbide-jwt-svid
}
```

Response:

```json
{
  "siteId": "uuid",
  "token_endpoint": "https://tenant.example.com/oauth2/token",
  "auth_method": "client_secret_basic",
  "client_id": "abc123",
  "client_secret": "super-secret",
  "subject_token_audience": "value"
}
```

Possible values for `auth_method`:

* `client_secret_basic` supported  
* `none` supported. if set, the client_id and client_secret are ignored/not passed to exchange endpoint
* `client_secret_post` currently unsupported  
* `private_key_jwt` currently unsupported  
* `mtls` currently unsupported

#### **3.5.1.3 Token Exchange Request**

Make a request to the `token_endpoint` passed in `/token/delegation` API.

**Request**:

```bash
POST https://tenant.example.com/oauth2/token
Content-Type: application/x-www-form-urlencoded

grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Atoken-exchange
&subject_token=...
&subject_token_type=urn%3Aietf%3Aparams%3Aoauth%3Atoken-type%3Ajwt
```

**Response**:

```bash
200 OK
Content-Type: application/json
Content-Length: ...
{
  "access_token":"...",
  "issued_token_type":
      "urn:ietf:params:oauth:token-type:jwt",
  "token_type":"Bearer",
  "expires_in": ...
 }
```

The exchange service serves an [RFC 8693](https://datatracker.ietf.org/doc/html/rfc8693) token exchange endpoint for swapping SPIRE issued SVIDs with a tenant specific issuer SVID or access token.

#### **3.5.1.4 SPIFFE JWKS Endpoint**

```bash
GET
https://{carbide-rest}/v2/org/{org-id}/carbide/site/{site-id}/.well-known/jwks.json

{
  "keys": [{
    "kty": "EC",
    "use": "sig",
    "crv": "P-256",
    "kid": "af6426a5-5f49-44b9-8721-b5294be20bb6",
    "x": "SM0yWlon_8DYeFdlYhOg1Epfws3yyL5X1n3bvJS1CwU",
    "y": "viVGhYhzcscQX9gRNiUVnDmQkvdMzclsQUtgeFINh8k",
    "alg": "ES256"
  }]
}
```

#### **3.5.1.5 OIDC Discovery URL**

```bash
GET
https://{carbide-rest}/v2/org/{org-id}/carbide/site/{site-id}/.well-known/openid-configuration

{
  "issuer": "https://{carbide-rest}/v2/org/{org-id}/carbide/site/{site-id}",
  "jwks_uri": "https://{carbide-rest}/v2/org/{org-id}/carbide/site/{site-id}/.well-known/jwks.json",
  "response_types_supported": [
    "id_token"
  ],
  "subject_types_supported": [
    "public"
  ],
  "id_token_signing_alg_values_supported": [
    "ES256",
    "ES384",
    "ES512",
    "EdDSA"
  ]
}
```

#### **3.5.1.6 HTTP Response Statuses**

**HTTP Method Success Response Matrix**

| Method | Possible Success Codes | Desc |
| ----- | ----- | ----- |
| GET | 200 OK | Resource exists, returned in body |
| GET | 404 Not Found | Resource not configured yet |
| PUT | 201 Created | Resource was newly created |
| PUT | 200 OK | Resource replaced/updated |
| DELETE | 204 No Content | Resource deleted successfully |
| DELETE | 404 Not Found (optional) | Resource did not exist |

**HTTP Error Codes**

| Scenario | Status |
| ----- | ----- |
| Invalid JSON | 400 Bad Request |
| Schema validation failure | 422 Unprocessable Entity |
| Unauthorized | 401 Unauthorized |
| Authenticated but no permission | 403 Forbidden |
| Conflict (e.g. immutable field change) | 409 Conflict |

### **3.5.2 Internal gRPC APIs**

```protobuf
syntax = "proto3";
// crates/rpc/proto/forge.proto

// Machine Identity - JWT-SVID token signing
message MachineIdentityRequest {
  repeated string audience = 1;
}

message MachineIdentityResponse {
  string access_token = 1;
  string issued_token_type = 2;
  string token_type = 3;
  string expires_in = 4;
}

// gRPC service
service Forge {
  // SPIFFE Machine Identity APIs
  // Signs a JWT-SVID token for machine identity, 
  // used by DPU agent meta-data (IMDS) service
  rpc SignMachineIdentity(MachineIdentityRequest) returns (MachineIdentityResponse);
}
```

```protobuf
syntax = "proto3";
// crates/rpc/proto/forge.proto

// Token Delegation config message
message TokenDelegation {
  string org_id = 1;
  string token_endpoint = 2;
  string auth_method = 3;
  string client_id = 4;
  string client_secret = 5; // write-only, never returned in responses
  string subject_token_audience = 6; // audience to include in Carbide JWT-SVID
  bool enabled = 7;
  google.protobuf.Timestamp created_at = 8;
  google.protobuf.Timestamp updated_at = 9;
}

// Request for GET / DELETE (identifies org and site)
message GetTokenDelegationRequest {
  string org_id = 1;
}

// Request for PUT (includes delegation config)
message PutTokenDelegationRequest {
  string org_id = 1;
  string token_endpoint = 2;
  string auth_method = 3;
  string client_id = 4;
  string client_secret = 5;
  string subject_token_audience = 6;
  bool enabled = 7;
}

// Response for GET / PUT
message TokenDelegationResponse {
  TokenDelegation delegation = 1;
}

// gRPC service
service Forge {

  // Token Delegation Endpoints
  rpc GetTokenDelegation(GetTokenDelegationRequest) returns (TokenDelegationResponse) {}
  rpc PutTokenDelegation(PutTokenDelegationRequest) returns (TokenDelegationResponse) {}
  rpc DeleteTokenDelegation(GetTokenDelegationRequest) returns (google.protobuf.Empty) {}
}
```

```protobuf
syntax = "proto3";
// crates/rpc/proto/forge.proto

// JWK (JSON Web Key)
message JWK {
  string kty = 1; // Key type, e.g., "EC" or "RSA"
  string use = 2; // Key usage, e.g., "sig"
  string crv = 3; // Curve name (EC)
  string kid = 4; // Key ID
  string x = 5; // Base64Url X coordinate (EC)
  string y = 6; // Base64Url Y coordinate (EC)
  string n = 7; // Modulus (RSA)
  string e = 8; // Exponent (RSA)
  string alg = 9; // Algorithm, e.g., "ES256", "RS256"
  google.protobuf.Timestamp created_at = 10; // Optional key creation time
  google.protobuf.Timestamp expires_at = 11; // Optional expiration
}

// JWKS response
message JWKS {
  repeated JWK keys = 1;
  uint32 version = 2; // Optional JWKS version
}

// OpenID Configuration
message OpenIDConfiguration {
  string issuer = 1;
  string jwks_uri = 2;
  repeated string response_types_supported = 3;
  repeated string subject_types_supported = 4;
  repeated string id_token_signing_alg_values_supported = 5;
  uint32 version = 6; // Optional config version
}

// Request message
message GetOpenIDConfigRequest {
  string org_id = 1;    // org-id
}

// gRPC service
service Forge {
  // OIDC .well-known Endpoints
  rpc GetJWKS(GetJWKSRequest) returns (JWKS) {}
  rpc GetOpenIDConfiguration(GetOpenIDConfigRequest) returns (OpenIDConfiguration) {}
}
```

### **3.5.2.1 Mapping REST \-\> gRPC** 

| REST Method & Endpoint | gRPC Method | Description |
| ----- | ----- | ----- |
| `GET /v2/org/{org-id}/carbide/site/{site-id}/.well-known/jwks.json` | `Forge.GetJWKS` | Fetch JSON Web Key Set |
| `GET /v2/org/{org-id}/carbide/site/{site-id}/.well-known/openid-configuration` | `Forge.GetOpenIDConfiguration` | Fetch OpenID Connect config |
| `GET /v2/org/{org-id}/carbide/site/{site-id}/token-delegation` | `Forge.GetTokenDelegation` | Retrieve token delegation config |
| `PUT /v2/org/{org-id}/carbide/site/{site-id}/token-delegation` | `Forge.PutTokenDelegation` | Create or replace token delegation |
| `DELETE /v2/org/{org-id}/carbide/site/{site-id}/token-delegation` | `Forge.DeleteTokenDelegation` | Delete token delegation |

### **3.5.2.2 Error Handling**

Use standard gRPC `Status` codes, aligned with REST:

| REST | gRPC Status | Notes |
| ----- | ----- | ----- |
| 400 Bad Request | `INVALID_ARGUMENT` | Malformed request |
| 401 Unauthorized | `UNAUTHENTICATED` | Invalid credentials |
| 403 Forbidden | `PERMISSION_DENIED` | Not allowed |
| 404 Not Found | `NOT_FOUND` | Resource missing |
| 409 Conflict | `ALREADY_EXISTS` | Immutable field conflicts |
| 500 Internal | `INTERNAL` | Unexpected server error |

# **4\. Technical Considerations**

## **4.1 Security**

1. All internal API gRPC calls to the Carbide API server use (existing) mTLS for authn/z and transport security. A future release also relies on attestation features.     
2. Carbide-rest is served over HTTPS and supports SSO integration  
3. The IMDS service is exposed over link-local and is exposed only to the node instance. Short-lived tokens (configurable TTL) limit the replay window. Adding Metadata: true HTTP header to the requests to limit SSRF attacks. In order to ensure that requests are directly intended for IMDS and prevent unintended or unwanted redirection of requests, requests:  
  * Must contain the header `Metadata: true`  
  * Must not contain an `X-Forwarded-For` header

  Any request that doesn't meet both of these requirements is rejected by the service. 

4. Requests to IMDS are limited to 3 requests per second. Requests exceeding this threshold will be rejected with 429 responses. This prevents DoS on DPU-agent and Carbide API server due to frequent IMDS calls.  
5. Input validation: The input such as machine id will be validated using the database before issuing the token.  
6. HTTPS and optional HTTP proxy support for route token exchange call to limit SSRF attacks on internal systems. 
