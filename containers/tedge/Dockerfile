FROM alpine:3.21 AS build
# Use a build layer to avoid adding binaries from every target into a layer into the output image
ARG TARGETPLATFORM

# Extract the appropriate thin-edge.io target based on the docker target platform
COPY packages/ /packages/
RUN case "${TARGETPLATFORM}" in \
         "linux/amd64")  RUST_TARGET=x86_64-unknown-linux-musl; APK_TARGET=x86_64  ;; \
         "linux/arm64")  RUST_TARGET=aarch64-unknown-linux-musl; APK_TARGET=aarch64 ;; \
         "linux/arm/v7") RUST_TARGET=armv7-unknown-linux-musleabihf; APK_TARGET=armv7  ;; \
         "linux/arm/v6") RUST_TARGET=arm-unknown-linux-musleabi; APK_TARGET=armhf  ;; \
         *) echo "Unsupported target platform: TARGETPLATFORM=$TARGETPLATFORM"; exit 1 ;; \
    esac \
    # install via the apk package as it includes additional files, e.g. plugins
    && find /packages -name "tedge_${TEDGE_VERSION:-"*"}_${APK_TARGET}.apk" -exec apk add --allow-untrusted {} \; \
    # Fail early if matching binaries are not found
    && ls -l /usr/bin/tedge*

#
# Output
#
FROM alpine:3.21

ARG USERNAME=tedge
ARG USER_UID=1000
ARG USER_GID=$USER_UID

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
COPY --from=build /usr/bin/tedge /usr/bin/
COPY --from=build /usr/share/tedge/config-plugins/* /usr/share/tedge/config-plugins/
COPY --from=build /usr/share/tedge/log-plugins/* /usr/share/tedge/log-plugins/
COPY --from=build /usr/share/tedge/diag-plugins/* /usr/share/tedge/diag-plugins/

# setup thin-edge.io
RUN addgroup -g "$USER_GID" -S "$USERNAME" \
    && adduser -u "$USER_UID" -g '' -H -D "$USERNAME" -G "$USERNAME" \
    && /usr/bin/tedge init \
    && /usr/bin/c8y-remote-access-plugin --init

# Add custom config
COPY system.toml /etc/tedge/

ENV TEDGE_MQTT_BIND_ADDRESS=0.0.0.0
ENV TEDGE_C8Y_PROXY_BIND_ADDRESS=0.0.0.0
ENV TEDGE_HTTP_BIND_ADDRESS=0.0.0.0
ENV TEDGE_MQTT_CLIENT_HOST=mqtt-broker
ENV TEDGE_HTTP_CLIENT_HOST=tedge
ENV TEDGE_C8Y_PROXY_CLIENT_HOST=tedge-mapper-c8y

USER "$USER_UID:$USER_GID"
CMD [ "/usr/bin/tedge-agent" ]
