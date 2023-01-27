# See here for image contents: https://github.com/microsoft/vscode-dev-containers/tree/v0.195.0/containers/rust/.devcontainer/base.Dockerfile
# [Choice] Debian OS version (use bullseye on local arm64/Apple Silicon): buster, bullseye
ARG VARIANT="bullseye"
FROM mcr.microsoft.com/devcontainers/rust:1-${VARIANT}

# [Optional] Uncomment this section to install additional packages.
RUN apt-get update && export DEBIAN_FRONTEND=noninteractive \
    && apt-get -y install --no-install-recommends \
        # apt dependencies
        apt-transport-https ca-certificates curl gnupg-agent software-properties-common lsb-release \
        # Debian packaging utilities
        equivs \
        dpkg-dev \
        fakeroot \
        # Enable systemd
        systemd \
        # Testing dependencies
        docker \
        python3 \
        python3-pip \
        # tedge dependencies
        mosquitto

# Install docker cli
RUN mkdir -p /etc/apt/keyrings \
    && curl -fsSL https://download.docker.com/linux/debian/gpg | sudo gpg --dearmor -o /etc/apt/keyrings/docker.gpg \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/debian $(lsb_release -cs) stable" | tee /etc/apt/sources.list.d/docker.list > /dev/null \
    && apt-get update \
    && apt-get install -y docker-ce-cli docker-compose-plugin

# Node js (for browser based tests)
RUN curl -fsSL https://deb.nodesource.com/setup_18.x | bash - \
    && apt-get install -y nodejs

# Update python deps
RUN pip3 install --upgrade pip \
    && pip3 install pipenv

# Delete the dummy systemctl command added by the base image
RUN rm -f /usr/local/bin/systemctl

# Install cargo packages under non-root user
USER vscode
RUN cargo install cargo-deb --version 1.41.3 \
    && cargo install cross

# Persist bash history
USER root
RUN SNIPPET="export PROMPT_COMMAND='history -a' && export HISTFILE=/commandhistory/.bash_history" \
    && mkdir /commandhistory \
    && touch /commandhistory/.bash_history \
    && echo "$SNIPPET" >> "/root/.bashrc"

# Enable cross-rs to compile using docker-in-docker
ENV CROSS_CONTAINER_IN_CONTAINER=true

CMD ["/lib/systemd/systemd", "--system", "--unit=basic.target"]
