# See here for image contents: https://github.com/microsoft/vscode-dev-containers/tree/v0.195.0/containers/rust/.devcontainer/base.Dockerfile
# [Choice] Debian OS version (use bullseye on local arm64/Apple Silicon): buster, bullseye
ARG VARIANT="bookworm"
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
        # Utilities
        vim \
        bash-completion \
        iputils-ping \
        # Enable systemd
        systemd \
        # Testing dependencies
        docker \
        python3 \
        python3-pip \
        python3-venv

# tedge dependencies
# Install more recent version of mosquitto >= 2.0.18 from debian sid to avoid mosquitto following bugs:
# The mosquitto repo can't be used as it does not included builds for arm64/aarch64 (only amd64 and armhf)
# * https://github.com/eclipse/mosquitto/issues/2604 (2.0.11)
# * https://github.com/eclipse/mosquitto/issues/2634 (2.0.15)
RUN sh -c "echo 'deb [signed-by=/usr/share/keyrings/debian-archive-keyring.gpg] http://deb.debian.org/debian sid main' > /etc/apt/sources.list.d/debian-sid.list" \
    && apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get -y --no-install-recommends install \
        mosquitto \
        mosquitto-clients \
    # Remove sid afterwards to prevent unexpected packages from being installed
    && rm -f /etc/apt/sources.list.d/debian-sid.list \
    && apt-get update

# Install gh utility
RUN curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg | sudo dd of=/usr/share/keyrings/githubcli-archive-keyring.gpg \
    && sudo chmod go+r /usr/share/keyrings/githubcli-archive-keyring.gpg \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" | sudo tee /etc/apt/sources.list.d/github-cli.list > /dev/null \
    && sudo apt update \
    && sudo apt install -y gh

# Install nfpm packager
RUN echo "deb [trusted=yes] https://repo.goreleaser.com/apt/ /" | tee /etc/apt/sources.list.d/goreleaser.list \
    && apt update \
    && apt install -y nfpm

# Install docker cli
RUN mkdir -p /etc/apt/keyrings \
    && curl -fsSL https://download.docker.com/linux/debian/gpg | sudo gpg --dearmor -o /etc/apt/keyrings/docker.gpg \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/debian $(lsb_release -cs) stable" | tee /etc/apt/sources.list.d/docker.list > /dev/null \
    && apt-get update \
    && apt-get install -y docker-ce-cli docker-compose-plugin

# Node js (for browser based tests and documentation using docusaurus)
RUN curl -fsSL https://deb.nodesource.com/setup_18.x | bash - \
    && apt-get install -y nodejs \
    && npm install -g yarn

# Instll just (project task runner)
RUN curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh | bash -s -- --to /usr/bin/

# Update python deps
RUN pip3 install --upgrade pip --break-system-packages

# Delete the dummy systemctl command added by the base image
RUN rm -f /usr/local/bin/systemctl

# Persist bash history
USER root
RUN SNIPPET="export PROMPT_COMMAND='history -a' && export HISTFILE=/commandhistory/.bash_history" \
    && mkdir /commandhistory \
    && touch /commandhistory/.bash_history \
    && echo "$SNIPPET" >> "/root/.bashrc" \
    # Enable bash-completion
    && echo "source /etc/profile.d/bash_completion.sh" >> "/root/.bashrc"

# Enable cross-rs to compile using docker-in-docker
ENV CROSS_CONTAINER_IN_CONTAINER=true

CMD ["/lib/systemd/systemd", "--system", "--unit=basic.target"]
