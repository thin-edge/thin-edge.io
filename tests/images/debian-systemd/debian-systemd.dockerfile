FROM debian:12-slim

# Install
RUN apt-get -y update \
    && DEBIAN_FRONTEND=noninteractive apt-get -y --no-install-recommends install \
    wget \
    curl \
    gnupg2 \
    sudo \
    apt-transport-https \
    ca-certificates \
    systemd \
    dbus \
    systemd-sysv \
    ssh \
    vim.tiny \
    nginx \
    netcat-openbsd \
    iputils-ping \
    net-tools \
    # json tools (for tests)
    jq \
    jo \
    # shells
    bash \
    bash-completion \
    zsh \
    fish \
    # mosquitto (default version used by Debian, see below for more details)
    mosquitto \
    mosquitto-clients

# Note: Avoid using mosquitto 2.0.18 due to a session persistence bug when using `per_listener_settings true
# Only comment out the custom install logic to make it easier to re-enable once the bug is resolved
# See https://github.com/thin-edge/thin-edge.io/issues/3185 for more details
# Install more recent version of mosquitto >= 2.0.18 from debian backports to avoid mosquitto following bugs:
# The mosquitto repo can't be used as it does not included builds for arm64/aarch64 (only amd64 and armhf)
# * https://github.com/eclipse/mosquitto/issues/2604 (2.0.11)
# * https://github.com/eclipse/mosquitto/issues/2634 (2.0.15)
# * https://github.com/eclipse/mosquitto/issues/2618 (2.0.18)
#RUN sh -c "echo 'deb [signed-by=/usr/share/keyrings/debian-archive-keyring.gpg] http://deb.debian.org/debian bookworm-backports main' > /etc/apt/sources.list.d/debian-bookworm-backports.list" \
#    && apt-get update \
#    && DEBIAN_FRONTEND=noninteractive apt-get -y --no-install-recommends install -t bookworm-backports \
#        mosquitto \
#        mosquitto-clients

# Remove unnecessary systemd services
RUN rm -f /lib/systemd/system/multi-user.target.wants/* \
    /etc/systemd/system/*.wants/* \
    /lib/systemd/system/local-fs.target.wants/* \
    /lib/systemd/system/sockets.target.wants/*udev* \
    /lib/systemd/system/sockets.target.wants/*initctl* \
    /lib/systemd/system/systemd-update-utmp* \
    # Remove policy-rc.d file which prevents services from starting
    && rm -f /usr/sbin/policy-rc.d

# Install base files to help with bootstrapping and common settings
WORKDIR /setup
COPY files/bootstrap.sh .
COPY files/system.toml /etc/tedge/
COPY files/tedge.toml /etc/tedge/tedge.toml
COPY files/tedge-configuration-plugin.toml /etc/tedge/plugins/
COPY files/packages/ /setup/packages/

COPY files/mqtt-logger.service /etc/systemd/system/
COPY files/mqtt-logger /usr/bin/
RUN systemctl enable mqtt-logger.service

# Custom mosquitto config
COPY files/mosquitto.conf /etc/mosquitto/conf.d/
COPY files/secure-listener.conf .

# Install nginx server to provide some dummy test files (e.g. with speed limiting options)
COPY files/http-server/nginx.conf /etc/nginx/nginx.conf
COPY files/http-server/*.sh /usr/bin/
RUN systemctl disable nginx

# Enable tab completions (note: fish does not require any changes)
RUN echo '[ -f /etc/bash_completion ] && source /etc/bash_completion' >> ~/.bashrc \
    && echo 'autoload -U compinit; compinit' >> ~/.zshrc \
    # zsh styling to make the completion menu easier to read and use
    && echo "zstyle ':completion:*' menu select" >> ~/.zshrc \
    && echo '# bind shift+tab to reverse menu complete' >> ~/.zshrc \
    && echo "zmodload zsh/complist" >> ~/.zshrc \
    && echo "bindkey -M menuselect '^[[Z' reverse-menu-complete" >> ~/.zshrc

# Reference: https://developers.redhat.com/blog/2019/04/24/how-to-run-systemd-in-a-container#enter_podman
# STOPSIGNAL SIGRTMIN+3 (=37)
STOPSIGNAL 37

CMD ["/lib/systemd/systemd"]
