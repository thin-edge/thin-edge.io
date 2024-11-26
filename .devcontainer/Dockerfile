# See here for image contents: https://github.com/microsoft/vscode-dev-containers/tree/v0.195.0/containers/rust/.devcontainer/base.Dockerfile
# [Choice] Debian OS version (use bullseye on local arm64/Apple Silicon): buster, bullseye
ARG VARIANT="bookworm"
FROM mcr.microsoft.com/devcontainers/rust:1-${VARIANT}

# [Optional] Uncomment this section to install additional packages.
RUN apt-get update && export DEBIAN_FRONTEND=noninteractive \
    && apt-get -y install --no-install-recommends \
        # apt dependencies
        apt-transport-https ca-certificates gnupg-agent software-properties-common \
        # Debian packaging utilities
        equivs \
        dpkg-dev \
        fakeroot \
        # Utilities
        ssh \
        sudo \
        gnupg2 \
        curl \
        wget \
        vim \
        bash-completion \
        netcat-openbsd \
        iputils-ping \
        net-tools \
        # Enable systemd
        systemd \
        systemd-sysv \
        dbus \
        # Testing dependencies
        docker \
        python3 \
        python3-pip \
        python3-venv \
        nginx \
        # JSON tools
        jq \
        jo \
    # tedge dependencies
    # Install more recent version of mosquitto >= 2.0.18 from debian backports to avoid mosquitto following bugs:
    # The mosquitto repo can't be used as it does not included builds for arm64/aarch64 (only amd64 and armhf)
    # * https://github.com/eclipse/mosquitto/issues/2604 (2.0.11)
    # * https://github.com/eclipse/mosquitto/issues/2634 (2.0.15)
    && sh -c "echo 'deb [signed-by=/usr/share/keyrings/debian-archive-keyring.gpg] http://deb.debian.org/debian bookworm-backports main' > /etc/apt/sources.list.d/debian-bookworm-backports.list" \
    && apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get -y --no-install-recommends install -t bookworm-backports \
        mosquitto \
        mosquitto-clients \
    #
    # Remove unnecessary systemd services
    && rm -f /lib/systemd/system/multi-user.target.wants/* \
        /etc/systemd/system/*.wants/* \
        /lib/systemd/system/local-fs.target.wants/* \
        /lib/systemd/system/sockets.target.wants/*udev* \
        /lib/systemd/system/sockets.target.wants/*initctl* \
        /lib/systemd/system/systemd-update-utmp* \
    #
    # Remove policy-rc.d file which prevents services from starting
    && rm -f /usr/sbin/policy-rc.d \
    #
    # Install gh utility
    && curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg | sudo dd of=/usr/share/keyrings/githubcli-archive-keyring.gpg \
    && sudo chmod go+r /usr/share/keyrings/githubcli-archive-keyring.gpg \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" | sudo tee /etc/apt/sources.list.d/github-cli.list > /dev/null \
    && sudo apt update \
    && sudo apt install -y gh \
    #
    # Install nfpm packager
    && echo "deb [trusted=yes] https://repo.goreleaser.com/apt/ /" | tee /etc/apt/sources.list.d/goreleaser.list \
    && apt update \
    && apt install -y nfpm \
    #
    # Install docker cli
    && mkdir -p /etc/apt/keyrings \
    && curl -fsSL https://download.docker.com/linux/debian/gpg | sudo gpg --dearmor -o /etc/apt/keyrings/docker.gpg \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/debian $(. /etc/os-release && echo "$VERSION_CODENAME") stable" | tee /etc/apt/sources.list.d/docker.list > /dev/null \
    && apt-get update \
    && apt-get install -y docker-ce-cli docker-compose-plugin docker-buildx-plugin \
    #
    # Node js (for browser based tests and documentation using docusaurus)
    && curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y nodejs \
    && npm install -g yarn \
    #
    # Install just (project task runner)
    && curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh | bash -s -- --to /usr/bin/ \
    #
    # Update python deps
    && pip3 install --upgrade pip --break-system-packages \
    #
    # Delete the dummy systemctl command added by the base image
    && rm -f /usr/local/bin/systemctl

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
