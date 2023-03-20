# How to the access test devices

The core thin-edge.io team conducts tests on real hardware to verify the functionality in a real world scenario. The devices which are used in the tests and how to connect to them (for the core thin-edge.io team only) is documented in the following sections.

## List of Devices available

The following table details the test devices which are available for use by the core team. These devices are only reachable via a OpenVPN connection, and instructions on setting the VPN connection up can be found in the following section.

|Hardware|IP|Username|Password|LSB|Arch|Comments|
|--------|--|--------|--------|---|---|--------|
| Raspberry Pi 4 | 192.168.1.1	 | `-` | `-` | Debian GNU/Linux 11 (bullseye) | aarch64 | Hosting Gateway and VPN Connection |
| Raspberry Pi 4 | 192.168.1.110 | `pi`| `thinedge` | Debian GNU/Linux 11 (bullseye) | aarch64 |
| Raspberry Pi 3 | 192.168.1.120 | `pi`| `thinedge` | Raspbian GNU/Linux 11 (bullseye)| armv71 |
| Raspberry Pi 4 | 192.168.1.130 | `pi`| `thinedge` | Raspbian GNU/Linux 11 (bullseye) | armv71 |
| Raspberry Zero | 192.168.1.140 | `zero`| `thinedge` | Raspbian GNU/Linux 11 (bullseye) | armv6l |
| Raspberry Pi 3 | 192.168.1.150 | `pi`| `thinedge` | Debian GNU/Linux 11 (bullseye) | aarch64 |
| Raspberry PI 4 | 192.168.1.200 | `pi`| `thinedge`| Debian GNU/Linux 11 (bullseye)| aarch64 | For NonFunctional tests |

## Connecting with OpenVPN

The test devices are only reachable via a OpenVPN connection. Follow the instructions carefully to get setup. Once the connection has been established you should be able to ssh into the devices listed in the previous section.

### Installing OpenVPN Client

To install OpenVPN, please checkout the installation instructions. The following links are provided for convenience. If you have any problems please consult the OpenVPN documentation.

|Windows|Link|
|-------|----|
|Windows|[OpenVPN Connect Client on Windows](https://openvpn.net/vpn-server-resources/installation-guide-for-openvpn-connect-client-on-windows/)|
|Linux (Debian and Ubuntu)|[OpenVPN 3 Client for Linux](https://openvpn.net/cloud-docs/openvpn-3-client-for-linux/)|
|MacOS|[OpenVPN Connect Client on macOS](https://openvpn.net/vpn-server-resources/installation-guide-for-openvpn-connect-client-on-macos/)|


### Add OpenVPN Profile

1. Request a Profile file (*.ovpn)
2. Open the OpenVPN Connect app and click plus.
3. Click **Browse** and locate the previously downloaded OpenVPN profile.
4. Select the profile in the file directory click **Open** in the file explorer.
5. Click **Add** to import the OpenVPN profile.
