"""Project tasks"""

from pathlib import Path
import logging
import os
import platform
import sys
import shlex
import shutil
import subprocess
import time
import datetime

import invoke
from invoke import task
from dotenv import load_dotenv


class ColourFormatter(logging.Formatter):
    grey = "\x1b[38;20m"
    yellow = "\x1b[33;20m"
    red = "\x1b[31;20m"
    bold_red = "\x1b[31;1m"
    reset = "\x1b[0m"
    format = "%(levelname)-8s %(message)s (%(filename)s:%(lineno)d)"

    FORMATS = {
        logging.DEBUG: grey + format + reset,
        logging.INFO: grey + format + reset,
        logging.WARNING: yellow + format + reset,
        logging.ERROR: red + format + reset,
        logging.CRITICAL: bold_red + format + reset,
    }

    def format(self, record):
        log_fmt = self.FORMATS.get(record.levelno)
        formatter = logging.Formatter(log_fmt)
        return formatter.format(record)


output_path = Path(__file__).parent / "output"
project_dir = Path(__file__).parent.parent.parent
project_dotenv = project_dir.joinpath(".env")
test_image_file_dir = project_dir / "tests/images/debian-systemd/files/deb"

# LOG settings
log = logging.getLogger("invoke")
log.setLevel(logging.INFO)
ch = logging.StreamHandler()
ch.setLevel(logging.DEBUG)
ch.setFormatter(ColourFormatter())
log.addHandler(ch)


for item in [project_dotenv, ".env"]:
    if os.path.exists(item):
        load_dotenv(item, override=True)

# pylint: disable=invalid-name


# Test adapter types
ENV_DEVICE_ADAPTER = "DEVICE_ADAPTER"
ADAPTER_DOCKER = "docker"
ADAPTER_LOCAL = "local"
ADAPTER_SSH = "ssh"


def detect_container_cli():
    """Detect the container cli (e.g. docker, podman, nerdctl)"""
    container_cli_options = ["docker", "podman", "nerdctl"]

    # Check which ones are actually available
    available = [cli for cli in container_cli_options if shutil.which(cli)]

    if not available:
        raise FileNotFoundError(
            f"No container cli found. The following clis were checked: {container_cli_options}"
        )

    # Check if the DOCKER_HOST is configured, as this will control which endpoint is actually being used
    for cli in available:
        if cli in os.getenv("DOCKER_HOST", ""):
            return cli

    # Otherwise, use first available container cli
    return available[0]


def remove_duplicate_container_networks(cli: str, name: str):
    """Remove any networks with a duplicated network name.

    If duplicates are found, keep the first one and delete the others.

    Duplicate networks can sometimes occur due to some race conditions, see references:
        * https://github.com/moby/moby/issues/20648
        * https://github.com/moby/moby/issues/33561
    """
    network_ids = subprocess.check_output(
        [cli, "network", "ls", "--filter", f"name={name}", "-q"], text=True
    ).splitlines()

    if len(network_ids) > 1:
        log.warning(
            f"More than 1 network detected. Keep the first network and removing the rest"
        )
        for network_id in network_ids[1:]:
            try:
                log.info("Removing container network id: %s", network_id)
                subprocess.check_call([cli, "network", "rm", network_id.strip()])
            except subprocess.CalledProcessError as ex:
                log.warning(
                    "Could not delete container network. Trying to proceed anyway. error=%s",
                    ex,
                )


def is_ci():
    """Check if being run under ci"""
    return bool(os.getenv("CI"))


@task
def lint(c):
    """Run linter"""
    c.run(f"{sys.executable} -m pylint libraries")


@task(name="format")
def formatcode(c):
    """Format python code"""
    c.run(f"{sys.executable} -m black libraries")


@task(name="reports")
def start_server(c, port=9000):
    """Start simple webserver used to display the test reports"""
    print("Starting local webserver: \n\n", file=sys.stderr)
    path = str(output_path)
    print(
        f"   Go to the reports in your browser: http://localhost:{port}/log.html\n\n",
        file=sys.stderr,
    )
    c.run(f"{sys.executable} -m http.server {port} --directory '{path}'")


def detect_target():
    uname = str(platform.uname()[4]).casefold()
    arch = None
    if "arm64" in uname or "aarch64" in uname:
        arch = "aarch64-unknown-linux"
    elif "armv7" in uname:
        arch = "armv7-unknown-linux"
    elif "armv6" in uname:
        arch = "arm-unknown-linux"
    elif "x86_64" in uname or "amd64" in uname:
        arch = "x86_64-unknown-linux"

    return arch


@task
def clean(c):
    """Remove any debian files"""
    log.info(
        "Removing any existing files from %s",
        test_image_file_dir.relative_to(project_dir),
    )
    for filename in os.listdir(test_image_file_dir):
        # ignore hidden files
        if filename.startswith("."):
            continue

        file_path = os.path.join(test_image_file_dir, filename)
        if os.path.isfile(file_path) or os.path.islink(file_path):
            os.unlink(file_path)
        elif os.path.isdir(file_path):
            shutil.rmtree(file_path)


@task(pre=[clean])
def use_local(c, arch="", package_type="deb"):
    """Copy locally built packages to the location which is used by the test container image

    It will automatically delete any existing files in the destination directory

    Examples:
        invoke use-local
        # Copy any debian files from the locally built on your host's target architecture

        invoke use-local --arch "aarch"
        # Copy any debian files from the locally built *aarch* target folder

        invoke use-local --arch "aarch*musl"
        # Copy any debian files from aarch*musl target files
    """
    source_dir = project_dir / "target"
    dest_dir = test_image_file_dir

    if not arch:
        arch = detect_target()
        if not arch:
            log.error(
                "Could not detect your architecture. Please manually specify it using the --arch <value> flag"
            )
            sys.exit(1)
        log.info("Using auto detected target: %s", arch)

    source_files = []
    for path in source_dir.glob(f"*{arch}*"):
        source_files = list(path.rglob(f"*.{package_type}"))
        if path.is_dir() and len(source_files):
            source_dir = path
            break

    if not source_files:
        log.error(
            "Did not find any %s files under %s",
            arch,
            source_dir.relative_to(project_dir),
        )
        sys.exit(1)

    log.info(
        "Copying *.%s file/s from %s to %s%s",
        package_type,
        source_dir.relative_to(project_dir),
        dest_dir.relative_to(project_dir),
        os.path.sep,
    )
    for file in source_files:
        shutil.copy(file, dest_dir)


@task(
    name="build",
    help={
        "name": ("Output image name"),
        "cache": ("Don't use docker cache"),
        "local": ("Use a locally built packages for the current host target"),
        "binary": (
            "Binary to be used to build the container image. e.g. docker, podman. Defaults to docker"
        ),
        "build_options": (
            "Additional build options which will be passed directly to the binary building the image"
        ),
    },
)
def build(
    c, name="debian-systemd", cache=True, local=False, binary=None, build_options=""
):
    """Build the container integration test image

    Docker is used by default, unless if the DOCKER_HOST variable is pointing to podman
    and podman is installed.

    Examples:

        invoke build
        # Build the test container image (it will use any debian files that have already been copied to the files dir)

        invoke clean build
        # Build the test container image but remove any existing files

        invoke build --local
        # Build the test container image using the locally build version (auto detecting your host's architecture)

        invoke use-local --arch "aarch64" build
        # Build the test container image using the locally build version (auto detecting your)

        invoke use-local --arch "x86_64" build
        # Use locally built x86_64 images then build container image

    """

    if local:
        clean(c)
        use_local(c)

    # Support podman, and automatically switch if the DOCKER_HOST is set
    binary = binary or "docker"
    if shutil.which("podman") and "podman" in os.getenv("DOCKER_HOST", ""):
        binary = "podman"

    options = ""
    if not cache:
        options += " --no-cache"

    if build_options:
        options += f" {build_options}"

    context = "../images/debian-systemd"
    c.run(
        f"{binary} build -t {name} -f {context}/debian-systemd.dockerfile {options} {context}",
        echo=True,
    )


test_common_help = {
    "outputdir": ("Output directory where the reports will be saved to"),
    "processes": ("Number of processes to use when running tests"),
    "suite": ("Only run suites matching the given text"),
    "test_name": ("Only run tests matching the given text"),
    "include": ("Only run tests matching the given tag"),
    "exclude": ("Don't run tests matching the given tag"),
    "retries": ("Max global retries to execute on failed tests. Defaults to 0"),
    "adapter": (
        "Default device adapter to use to run tests. e.g. docker, ssh or local"
    ),
}


@task(
    help={
        "iterations": ("Number of test iterations to run"),
        **test_common_help,
    }
)
def flake_finder(
    c,
    iterations=2,
    suite="",
    test_name="",
    adapter="docker",
    retries=0,
    outputdir="output_flake_finder",
    processes=None,
    include="",
    exclude="",
):
    """Run tests multiple times to find any flakey tests

    The output directory should not exist prior to running the tests.

    Examples
        invoke flake-finder --iterations 100 --outputdir output_flake_finder_100
        # Run all integration tests 100 times and generate a report

        invoke flake-finder --iterations 2 --outputdir output_flake_finder --suite service_monitoring
        # Only run tests related to a given suite and run it 2 times
    """
    passed = []
    failed = []
    duration_start = time.time()
    for i in range(1, iterations + 1):
        iteration_output = Path(outputdir) / f"iteration-{i}"
        os.makedirs(iteration_output)
        try:
            test(
                c,
                outputdir=iteration_output,
                suite=suite,
                test_name=test_name,
                adapter=adapter,
                retries=retries,
                processes=processes,
                include=include,
                exclude=exclude,
            )
            passed.append(i)
        except invoke.exceptions.Failure as ex:
            failed.append(i)

    duration_sec = time.time() - duration_start

    print("\n\n" + "-" * 30)

    overall_result = "PASSED"
    if failed:
        overall_result = "FAILED"

    print(f"Overall: {overall_result}")
    print(
        f"Results: {iterations} iterations, {len(passed)} passed, {len(failed)} failed"
    )
    print(f"Elapsed time: {datetime.timedelta(seconds=duration_sec)}")

    if failed:
        print(f"Failed iterations: {failed}")


@task(
    aliases=["tests"],
    help=test_common_help,
)
def test(
    c,
    suite="",
    test_name="",
    adapter="docker",
    retries=0,
    outputdir=None,
    processes=None,
    include="",
    exclude="",
):
    """Run tests

    Examples

        invoke test
        # run all tests

        invoke test --test-name "Successful shell command with output" --processes 1
        # Run any test cases matching a give string

        invoke test --suite "shell_operation" --processes 1
        # Run suites matching a specific name

        invoke test --include "theme:troubleshooting AND theme:c8y" --processes 10
        # Run tests which includes specific tags
    """
    processes = int(processes or 10)

    if not outputdir:
        outputdir = output_path

    env_file = ".env"
    if env_file:
        log.info("loading .env file. path=%s", env_file)
        load_dotenv(env_file, verbose=True, override=True)

    if adapter:
        os.environ[ENV_DEVICE_ADAPTER] = adapter

    if adapter == ADAPTER_DOCKER:
        container_cli = detect_container_cli()
        network_name = "inttest-network"

        # create docker network that is used by each container
        # Create before launching tests otherwise there will be a race condition
        # which causes multiple networks with the same name to be created.
        log.info("Creating a container network")
        c.run(
            f"command -v {container_cli} &>/dev/null && ({container_cli} network create {network_name} --driver bridge || true) || true"
        )

        # Required because of docker race condition which leads to duplicate networks
        remove_duplicate_container_networks(container_cli, network_name)

    elif adapter in [ADAPTER_SSH, ADAPTER_LOCAL]:
        # Parallel processing is not supported when using ssh or local
        # as the same device is being used for each test
        processes = 1

    if processes == 1:
        # Use robot rather than pabot if only 1 process is being used
        command = [
            sys.executable,
            "-m",
            "robot.run",
            "--outputdir",
            str(outputdir),
            # Support optional retry on failed (for tests with specific Tags, e.g. "test:retry(2)")
            "--listener",
            f"RetryFailed:{retries}",
        ]
    else:
        # Use pabot to handle multiple processes
        command = [
            sys.executable,
            "-m",
            "pabot.pabot",
            "--processes",
            str(processes),
            "--outputdir",
            str(outputdir),
            # Support optional retry on failed (for tests with specific Tags, e.g. "test:retry(2)")
            "--listener",
            f"RetryFailed:{retries}",
        ]

    # include tags
    if include:
        command.extend(
            [
                "--include",
                shlex.quote(include),
            ]
        )

    # exclude tags
    if exclude:
        command.extend(
            [
                "--exclude",
                shlex.quote(exclude),
            ]
        )

    # suite filter
    if suite:
        command.extend(
            [
                "--suite",
                shlex.quote(suite),
            ]
        )

    # test filter
    if test_name:
        command.extend(
            [
                "--test",
                shlex.quote(test_name),
            ]
        )

    if not is_ci():
        command.extend(
            [
                "--consolecolors",
                "on",
                "--consolemarkers",
                "on",
            ]
        )

    command.append("tests")

    log.info(" ".join(command))
    c.run(" ".join(command))
