FROM debian:11-slim

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
    ssh \
    mosquitto \
    mosquitto-clients \
    vim.tiny

# Remove unnecessary systemd services
RUN rm -f /lib/systemd/system/multi-user.target.wants/* \
    /etc/systemd/system/*.wants/* \
    /lib/systemd/system/local-fs.target.wants/* \
    /lib/systemd/system/sockets.target.wants/*udev* \
    /lib/systemd/system/sockets.target.wants/*initctl* \
    /lib/systemd/system/sysinit.target.wants/systemd-tmpfiles-setup* \
    /lib/systemd/system/systemd-update-utmp* \
    # Remove policy-rc.d file which prevents services from starting
    && rm -f /usr/sbin/policy-rc.d

# Install base files to help with bootstrapping and common settings
WORKDIR /setup
COPY files/bootstrap.sh .
COPY files/system.toml /etc/tedge/
COPY files/c8y-configuration-plugin.toml /etc/tedge/c8y/
COPY files/deb/ /setup/deb/

COPY files/mqtt-logger.service /etc/systemd/system/
COPY files/mqtt-logger /usr/bin/
RUN systemctl enable mqtt-logger.service

# Custom mosquitto config
COPY files/mosquitto.conf /etc/mosquitto/conf.d/
COPY files/secure-listener.conf .

# Reference: https://developers.redhat.com/blog/2019/04/24/how-to-run-systemd-in-a-container#enter_podman
# STOPSIGNAL SIGRTMIN+3 (=37)
STOPSIGNAL 37

CMD ["/lib/systemd/systemd"]
