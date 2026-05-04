# Runtime image for forge-dpu-otel-agent on arm64 DPUs.
#
# The binary is cross-compiled externally using the
# build-artifacts-container-cross-aarch64 image and copied into this
# minimal runtime container.

ARG CC_VERSION=v3.2.1 # NVIDIA Distroless Container version
ARG TAG=${CC_VERSION}
ARG IMG=nvcr.io/nvidia/distroless/cc
ARG DEBUG_DISTROLESS=true
ARG DISTROLESS_DEBUG_TAG=${DEBUG_DISTROLESS:+"dev"} # 'dev' provides a busybox shell

FROM debian:12-slim AS udev
RUN apt-get update && apt-get install -y --no-install-recommends \
    libudev1

# NVIDIA Distroless Container
FROM ${IMG}:${TAG}-${DISTROLESS_DEBUG_TAG}

COPY --chmod=755 target/aarch64-unknown-linux-gnu/release/forge-dpu-otel-agent /usr/bin/forge-dpu-otel-agent

COPY --from=udev /lib/aarch64-linux-gnu/libudev.so.1 /lib/aarch64-linux-gnu/libudev.so.1

ENTRYPOINT ["/usr/bin/forge-dpu-otel-agent"]
CMD ["--config-path=/etc/forge-dpu-otel-agent/config.toml", "run" ]
