# See here for image contents: https://github.com/devcontainers/images/tree/main/src/rust/history
ARG VARIANT="trixie"
FROM mcr.microsoft.com/devcontainers/rust:2-${VARIANT}

# [Optional] Uncomment this section to install additional packages.
RUN apt-get update && export DEBIAN_FRONTEND=noninteractive \
    && apt-get -y install --no-install-recommends \
        # apt dependencies
        apt-transport-https ca-certificates gnupg-agent \
        # Debian packaging utilities
        equivs \
        dpkg-dev \
        fakeroot \
        # Build tools
        libclang-dev \
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
        python3 \
        python3-pip \
        python3-venv \
        nginx \
        # JSON tools
        jq \
        jo \
        # tedge dependencies
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
    && apt-get install -y docker-ce docker-ce-cli containerd.io docker-compose-plugin docker-buildx-plugin \
    #
    # Node js (for browser based tests and documentation using docusaurus)
    && curl -fsSL https://deb.nodesource.com/setup_24.x | bash - \
    && apt-get install -y nodejs \
    && npm install -g yarn \
    #
    # Install just (project task runner)
    && curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh | bash -s -- --to /usr/bin/ \
    #
    #
    # Delete the dummy systemctl command added by the base image
    && rm -f /usr/local/bin/systemctl \
    #
    # Install rust tools
    && rustup component add rust-analyzer rust-analysis rust-src rustfmt clippy \
    && rustup toolchain install nightly \
    && rustup component add rustfmt --toolchain nightly \
    # Use cargo-bininstall to improve multi-arch builds
    && curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash \
    && cargo-binstall cargo-nextest --secure \
    && cargo-binstall taplo-cli --secure \
    && cargo-binstall cargo-deny --secure

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
