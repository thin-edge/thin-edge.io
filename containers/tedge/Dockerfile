FROM alpine:3.18 as build
# Use a build layer to avoid adding binaries from every target into a layer into the output image
ARG TARGETPLATFORM

# Extract the appropriate thin-edge.io target based on the docker target platform
COPY packages/ /packages/
RUN case "${TARGETPLATFORM}" in \
         "linux/amd64")  RUST_TARGET=x86_64-unknown-linux-musl  ;; \
         "linux/arm64")  RUST_TARGET=aarch64-unknown-linux-musl ;; \
         "linux/arm/v7") RUST_TARGET=armv7-unknown-linux-musleabihf  ;; \
         "linux/arm/v6") RUST_TARGET=arm-unknown-linux-musleabihf  ;; \
         *) echo "Unsupported target platform: TARGETPLATFORM=$TARGETPLATFORM"; exit 1 ;; \
    esac \
    && mkdir -p /build \
    && find /packages -name "tedge_${TEDGE_VERSION:-"*"}_${RUST_TARGET}.tar.gz" -exec tar xzvf {} -C /build/ \; \
    # Fail early if matching binaries are not found
    && ls -l /build/tedge*

#
# Output
#
FROM alpine:3.18

ARG BUILDTIME
ARG REVISION
ARG VERSION

LABEL org.opencontainers.image.title="thin-edge.io"
LABEL org.opencontainers.image.created=$BUILDTIME
LABEL org.opencontainers.image.version=$VERSION
LABEL org.opencontainers.image.revision=$REVISION
LABEL org.opencontainers.image.description="thin-edge.io container with the multi-call binary"
LABEL org.opencontainers.image.source="https://github.com/thin-edge/thin-edge.io"
LABEL org.opencontainers.image.authors="thin-edge.io"
LABEL org.opencontainers.image.vendor="thin-edge.io"
LABEL org.opencontainers.image.licenses="Apache 2.0"
LABEL org.opencontainers.image.url="https://thin-edge.io"
LABEL org.opencontainers.image.documentation="https://thin-edge.github.io/thin-edge.io/"
LABEL org.opencontainers.image.base.name="alpine:3.18"

# Note: having ca-certificates is generally very useful as it enable trusted
# communication with https/mqtt servers, otherwise the users would always have
# to add their own certs into the image before they can use it
RUN apk add --no-cache \
    ca-certificates

# install thin-edge.io
COPY --from=build build/* /usr/bin/

# setup thin-edge.io
RUN addgroup -S tedge \
    && adduser -g '' -H -D tedge -G tedge \
    && /usr/bin/tedge init

# Add custom config
COPY system.toml /etc/tedge/

ENV TEDGE_MQTT_BIND_ADDRESS 0.0.0.0
ENV TEDGE_C8Y_PROXY_BIND_ADDRESS 0.0.0.0
ENV TEDGE_HTTP_BIND_ADDRESS 0.0.0.0
ENV TEDGE_MQTT_CLIENT_HOST mqtt-broker
ENV TEDGE_HTTP_CLIENT_HOST tedge
ENV TEDGE_C8Y_PROXY_CLIENT_HOST tedge-mapper-c8y

USER "tedge"
CMD [ "/usr/bin/tedge-agent" ]
