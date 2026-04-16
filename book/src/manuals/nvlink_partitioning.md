# NVLink Partitioning

NVIDIA [NVLink](https://www.nvidia.com/en-us/data-center/nvlink/) is a high-speed interconnect technology that allows for memory-sharing between GPUs. Sharing
is allowed between all GPUs in an *NVLink Partition*. An *NVLink Partition* must consist of GPUs within the same *NVLink Domain*, which can be a single NVL72 rack or two NVL36 racks cabled together.

NCX Infra Controller (NICo) allows you to do the following with NVLink:

* Create, update, and delete NVLink Logical Partitions using the NICo REST API.
* Provision Instances with GPUs partitioned into NVLink Domains without knowledge of the underlying NVLink topology.
* Monitor NVLink Partition status using telemetry.

NICo extends the concept of an *NVLink Partition* with the *NVLink Logical Partition* concept, which allows users to manage NVLink Partitions without knowing the datacenter topology.
NICo users can utilize *NVLink Logical Partitions* during Instance creation, as described in the following sections.

> **Note**: The following steps only apply to creating instances for GB200 compute nodes.

### Creating a NVLink Logical Partition

NICo users can create NVLink Logical Partitions and manually assign NVLink Interfaces for Instances (as described in steps **1-2**). NICo can also automatically generate NVLink Interfaces and assign them to Instances (as described in step **3**).

1. The user creates a NVLink Logical Partition using the `POST /v2/org/{org}/carbide/nvlink-logical-partition` [REST API endpoint](https://nvidia.github.io/ncx-infra-controller-rest/#tag/NVLink-Logical-Partition/operation/create-nvlink-logical-partition). NICo creates an entry in the database and returns a NVLink Logical Partition ID. At this point, there is no underlying NVLink Partition associated with the NVLink Logical Partition.

2. When creating an Instance, the user can specify NVLink Interface configuration for each GPU by referencing their preferred NVLink Logical Partition ID in the `POST /v2/org/{org}/carbide/instance` [REST API endpoint request](https://nvidia.github.io/ncx-infra-controller-rest/#tag/Instance/operation/create-instance).

   a. If this is the first Instance to be added to specified NVLink Logical Partitions, NICo will create and assign an NVLink Partition for them and add the Instance GPUs to the NVLink Partition.

> **Note**: To ensure that machines in the same rack are assigned to the same NVLink Partition, an Instance Type can be created for the rack and all Machines in the rack assigned to the same Instance Type. Alternatively users can use the [Batch Instance creation REST API endpoint](https://nvidia.github.io/ncx-infra-controller-rest/#tag/Instance/operation/batch-create-instances).

3. If the users does not want to specify NVLink Interfaces for each GPU when creating an Instance, they can:

   a. Create a newVPC with a default NVLink Logical Partition or update an existing VPC with no Instances to set a default NVLink Logical Partition.

   b. When creating an Instance in this VPC, user does not need to specify NVLink Interfaces, NICo will automatically generate NVLink Interfaces for the Instance and assign them to the default NVLink Logical Partition.

   c. All Instances created within this VPC will have their GPUs assigned to the same NVLink Partition as long as they are in the same Rack.

   d. If there is no space in the rack, NICo will create a new NVLink Partition in a different Rack for the same NVLink Logical Partition and continue to assign the Instance GPUs to it.

> **Important**: When NICo creates a new NVLink Partition within the same NVLink Logical Partition, the new Instance GPUs in the NVLink Logical Partition will not be able to share memory with the other Instances that were previously added to the NVLink Logical Partition.

### Removing Instances from a Logical Partition

If a NICo user de-provisions an Instance, NICo will remove the Instance GPUs from the NVLink Partition.

### Deleting a Logical Partition

A NICo user can call `DELETE /v2/org/{org}/carbide/nvlink-logical-partition/{nvLinkLogicalPartitionId}` to delete an NVLink Logical Partition. This call will only succeed if there are no physical partitions associated with the logical partition.

### Retrieving Partition Information for an Instance

A NICo user can call `GET /v2/org/{org}/carbide/instance/{instance-id}` to retrieve information about an instance. As part of the `200` response body, NICo will return a `nvLinkInterfaces` list that includes both the `nvLinkLogicalPartitionId` and `nvLinkDomainId` for each GPU in the Instance.

The `nvLinkDomainId` can be useful in some use cases. For example, when NICo is being used to provide Virtual Machines as a Service (VMaaS), Instances are created up front with no NVLink partition configured yet. Then, when a user spins up a virtual machine (VM), VMaaS schedules it on one of these Instances. Once the user has a group of VMs, they configure an NVLink partition. However, the Instances selected by VMaaS may all be in different NVLink domains, and won't be able to be added to a single partition. The NVLink Domain IDs can be used by the VMaaS to make an informed decision regarding where to schedule the VMs.
