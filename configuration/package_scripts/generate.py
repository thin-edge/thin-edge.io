#!/usr/bin/env python3
#
# Generate package scripts (aka maintainer scripts) used for linux packaging,
# e.g. post/pre install/remove
#
# The script processes a builder.json file which includes which packages it
# should build as well as which services it should also automatically add code
# snippets for if the '#LINUXHELPER#' tag is present. This concept is similar
# to the '#DEBHELPER#' [see docs](https://man7.org/linux/man-pages/man7/debhelper.7.html)
# It is intended to be more package type agnostic by supporting debian, rpm and alpine linux.
# Additional snippets can be added for future package managed types as required.
#
# Package script generation was chosen due to the large number of different packages in the project
# which makes it difficult to maintain consistency across all packages and services within
# each package.
#
# Usage:
#   python3 generate.py ./builder.json
#
##############################################################################################################

import json
import logging
import os
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Any

# Set sensible logging defaults
log = logging.getLogger()
log.setLevel(logging.INFO)
handler = logging.StreamHandler()
handler.setLevel(logging.INFO)
formatter = logging.Formatter("%(asctime)s - %(name)s - %(levelname)s - %(message)s")
handler.setFormatter(formatter)
log.addHandler(handler)


class JSONWithCommentsDecoder(json.JSONDecoder):
    """Enable parsing json with comments"""

    def __init__(self, **kw):
        super().__init__(**kw)

    def decode(self, s: str) -> Any:
        s = "\n".join(
            l if not l.lstrip().startswith("//") else "" for l in s.split("\n")
        )
        return super().decode(s)


@dataclass
class Service:
    name: str = ""
    enable: bool = True
    start: bool = True
    stop_on_upgrade: bool = True
    restart_after_upgrade: bool = True


def get_template(name, default=""):
    file = Path(name)
    if file.exists():
        return Path(name).read_text(encoding="utf8")
    return default


def replace_variables(
    contents: str, variables: Dict[str, str], wrap: bool = False
) -> str:
    expanded_contents = contents
    for key, value in variables.items():
        var_name = f"#{key}#".upper()
        expanded_contents = expanded_contents.replace(var_name, value)

    if wrap and expanded_contents:
        return "\n".join(
            [
                "# Automatically added by thin-edge.io",
                expanded_contents,
                "# End automatically added section",
            ]
        )
    return expanded_contents


def write_script(
    input_contents, lines: List[str], output_file: Path, debug: bool = True
) -> str:
    # filter out lines with only whitespace
    lines = [line for line in lines if line.strip()]
    contents = replace_variables(
        input_contents,
        {
            "LINUXHELPER": "\n".join(lines),
        },
        wrap=False,
    )

    if debug:
        print(f"---- start {output_file} ----\n")
        print(contents)
        print(f"---- end {output_file} ----\n")

    output_file.write_text(contents, encoding="utf8")
    return contents


def format_unit_name(name: str, default_suffix=".service") -> str:
    if "." not in name:
        return name + default_suffix
    return name


def process_package(name: str, manifest: dict, package_type: str, out_dir: Path):
    services = [Service(**service) for service in manifest.get("services", [])]

    postinst = []
    preinst = []
    prerm = []
    postrm = []

    service_names = [
        format_unit_name((service.name or name), ".service") for service in services
    ]
    log.info("Processing package: %s, type=%s", name, package_type)

    variables = {
        "UNITFILES": " ".join(service_names),
    }

    service = None

    for service in services:
        service_name = format_unit_name((service.name or name), ".service")
        log.info(
            "Processing service: package=%s, service=%s, type=%s",
            name,
            service_name,
            package_type,
        )

        variables["UNITFILE"] = service_name

        # The logic is derived from the cargo-deb logic which was previously
        # used by thin-edge.io to build the debian packages.
        # https://github.com/kornelski/cargo-deb/blob/main/src/dh_installsystemd.rs

        # postinst
        snippet = {
            True: "postinst-systemd-enable",
            False: "postinst-systemd-dont-enable",
        }[service.enable]
        postinst.append(
            replace_variables(
                get_template(f"templates/{package_type}/{snippet}"),
                variables,
                wrap=True,
            )
        )

    #
    # Group services files based on their properties as they grouped to be executed as a single command
    # The following template make use of UNITFILES, but since each package can contain multiple files
    # the services functionality needs to be grouped.
    #
    if services:
        # Helper
        def append_matching_services(
            script: List, template: str, predicate, variables: Dict
        ):
            matches = [
                format_unit_name((service.name or name), ".service")
                for service in services
                if predicate(service)
            ]
            if matches:
                script.append(
                    replace_variables(
                        get_template(f"templates/{package_type}/{template}"),
                        {
                            **variables,
                            "UNITFILES": " ".join(matches),
                        },
                        wrap=True,
                    )
                )

        # postrm
        postrm.append(
            replace_variables(
                get_template(f"templates/{package_type}/postrm-systemd-reload-only"),
                variables,
                wrap=True,
            )
        )

        # postrm
        # Special case for rpm packages:
        # By default rpm maintainer scripts restart mark a service to be restarted in the postrm script
        # unlike debian which does this in the postinst.
        if package_type == "deb":
            append_matching_services(
                postrm, "postrm-systemd", lambda x: True, variables
            )
        elif package_type == "rpm":
            append_matching_services(
                postrm, "postrm-systemd", lambda x: service.stop_on_upgrade, variables
            )

        ## postinst: restart after upgrade and start
        append_matching_services(
            postinst,
            "postinst-systemd-restart",
            lambda x: x.restart_after_upgrade and x.start,
            {
                **variables,
                "RESTART_ACTION": "restart",
            },
        )

        ## postinst: restart after upgrade and no start
        append_matching_services(
            postinst,
            "postinst-systemd-restartnostart",
            lambda x: x.restart_after_upgrade and not x.start,
            {
                **variables,
                "RESTART_ACTION": "try-restart",
            },
        )

        # postinst: restart_after_upgrade=false and start=true
        append_matching_services(
            postinst,
            "postinst-systemd-start",
            lambda x: not x.restart_after_upgrade and x.start,
            variables,
        )

        # prerm: stop_on_upgrade=false or restart_after_upgrade=true
        append_matching_services(
            prerm,
            "prerm-systemd-restart",
            lambda x: not x.stop_on_upgrade or x.restart_after_upgrade,
            variables,
        )

        # prerm: always stop service
        append_matching_services(
            prerm,
            "prerm-systemd",
            lambda x: not (not x.stop_on_upgrade or x.restart_after_upgrade)
            and x.start,
            variables,
        )

    # Default script contents if the script does not already exist
    default_t = "\n".join(
        [
            "#!/bin/sh",
            "set -e",
            "#LINUXHELPER#",
        ]
    )

    write_script(
        get_template(f"./{name}/postinst", default_t), postinst, out_dir / "postinst"
    )
    write_script(
        get_template(f"./{name}/postrm", default_t), postrm, out_dir / "postrm"
    )
    write_script(get_template(f"./{name}/prerm", default_t), prerm, out_dir / "prerm")
    write_script(
        get_template(f"./{name}/preinst", default_t), preinst, out_dir / "preinst"
    )


def main(file):
    manifests = json.loads(Path(file).read_text("utf8"), cls=JSONWithCommentsDecoder)
    packages = manifests.get("packages", {})
    package_types = manifests.get("types", [])

    output_dir = Path("_generated")
    output_dir.mkdir(parents=True, exist_ok=True)

    for name, manifest in packages.items():
        for package_type in package_types:
            package_dir = output_dir / name / package_type
            package_dir.mkdir(parents=True, exist_ok=True)
            process_package(name, manifest, package_type, package_dir)

    log.info("Successfully generated package scripts")


if __name__ == "__main__":
    # Change to script's directory so that relative paths
    # can be used for when generating the maintainer scripts
    os.chdir(str(Path(__file__).parent))
    main("packages.json" if len(sys.argv) < 2 else sys.argv[1])
