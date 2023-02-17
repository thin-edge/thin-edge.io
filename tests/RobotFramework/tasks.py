"""Project tasks"""

from pathlib import Path
import os
import sys
import shlex
from invoke import task

from dotenv import load_dotenv

output_path = Path(__file__).parent / "output"
project_dir = Path(__file__).parent.parent.parent
project_dotenv = project_dir.joinpath(".env")

for item in [project_dotenv, ".env"]:
    if os.path.exists(item):
        load_dotenv(item, override=True)

# pylint: disable=invalid-name


# Test adapter types
ENV_DEVICE_ADAPTER = "DEVICE_ADAPTER"
ADAPTER_DOCKER = "docker"
ADAPTER_LOCAL = "local"
ADAPTER_SSH = "ssh"


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


@task(name="build")
def build(c, name="debian-systemd"):
    """Build the docker integration test image"""
    context = "../images/debian-systemd"
    c.run(f"docker build -t {name} -f {context}/debian-systemd.dockerfile {context}", echo=True)


@task(
    help={
        "file": ("Robot file or directory to run"),
        "outputdir": ("Output directory where the reports will be saved to"),
        "processes": ("Number of processes to use when running tests"),
        "suite": ("Only run suites matching the given text"),
        "test": ("Only run tests matching the given text"),
        "include": ("Only run tests matching the given tag"),
        "exclude": ("Don't run tests matching the given tag"),
        "retries": ("Max global retries to execute on failed tests. Defaults to 0"),
        "adapter": ("Default device adapter to use to run tests. e.g. docker, ssh or local"),
    },
)
def test(c, file="tests", suite="", test="", adapter="docker", retries=0, outputdir=None, processes=None, include="", exclude=""):
    """Run tests

    Examples

        # run all tests
        invoke test

        # Run only tests defined in tests/myfile.robot
        invoke test --file=tests/myfile.robot
    """
    if not processes:
        processes = 10

    if not outputdir:
        outputdir = output_path

    env_file = ".env"
    if env_file:
        print(f"loading .env file. path={env_file}")
        load_dotenv(env_file, verbose=True, override=True)

    if adapter:
        os.environ[ENV_DEVICE_ADAPTER] = adapter

    if adapter == ADAPTER_DOCKER:
        # create docker network that is used by each container
        # Create before launching tests otherwise there will be a race condition
        # which causes multiple networks with the same name to be created.
        c.run("command -v docker &>/dev/null && (docker network create inttest-network --driver bridge || true) || true")
    elif adapter in [ADAPTER_SSH, ADAPTER_LOCAL]:
        # Parallel processing is not supported when using ssh or local
        # as the same device is being used for each test
        processes = 1

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
        command.extend([
            "--suite",
            shlex.quote(suite),
        ])

    # test filter
    if test:
        command.extend([
            "--test",
            shlex.quote(test),
        ])

    if not is_ci():
        command.extend(
            [
                "--consolecolors",
                "on",
                "--consolemarkers",
                "on",
            ]
        )

    if file:
        command.append(file)

    print(" ".join(command))
    c.run(" ".join(command))
