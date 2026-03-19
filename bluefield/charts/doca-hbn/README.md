# doca-hbn

Vendored copy of the upstream NVIDIA DOCA HBN (Host-Based Networking) Helm chart
from the NGC registry (`https://helm.ngc.nvidia.com/nvidia/doca`).

- **Chart version**: 1.0.5
- **App version**: 3.2.1-doca3.2.1
- **Source**: https://catalog.ngc.nvidia.com/orgs/nvidia/teams/doca/helm-charts/doca-hbn
- **Upstream reference**: https://github.com/NVIDIA/doca-platform (v25.10.1)

## Carbide modifications

The following values were added to `daemonset.yaml` and `values.yaml` to support
injecting arbitrary init containers, volume mounts, and volumes without forking
the rest of the template:

| Value | Description |
|---|---|
| `extraInitContainers` | List of init containers inserted before the upstream `hbn-init` container |
| `extraVolumeMounts` | List of volume mounts appended to the `doca-hbn` container |
| `extraVolumes` | List of volumes appended to the pod spec |

All three default to `[]`.

### Example

Use an init container to write a config file into a shared `emptyDir` volume,
then mount that volume into the `doca-hbn` container:

```yaml
extraInitContainers:
  - name: write-config
    image: busybox:latest
    command:
      - sh
      - -c
      - |
        cat <<'EOF' > /shared/my-config.yaml
        key: value
        setting: enabled
        EOF
    volumeMounts:
      - name: shared-data
        mountPath: /shared

extraVolumeMounts:
  - name: shared-data
    mountPath: /shared

extraVolumes:
  - name: shared-data
    emptyDir: {}
```
