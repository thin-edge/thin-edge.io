import os.path
import pysys
from pysys.basetest import BaseTest


class DockerPlugin(BaseTest):

    # Static class member that can be overriden by a command line argument
    # E.g.:
    # pysys.py run 'apt_*' -XmyPlatform='container'
    myPlatform = None
    docker_plugin = "/etc/tedge/sm-plugins/docker"
    docker_cmd = "docker"
    sudo = "/usr/bin/sudo"

    containers_to_clean = []
    images_to_clean = []

    def setup(self):
        if self.myPlatform != "container":
            self.skipTest(
                "Testing the docker plugin is not supported on this platform."
                + "To run it, all the test with -XmyPlatform='container'"
            )

        if not os.path.exists("/etc/tedge/sm-plugins/docker"):
            raise SystemError("Docker plugin missing")

        # Register routines to cleanup containers and images added during test
        self.addCleanupFunction(self.cleanup_images)
        self.addCleanupFunction(self.cleanup_containers)

    def assert_container_running(
        self, image_name, image_version=None, negate=False, abortOnError=False
    ):
        """Asserts that a container with the given image name and version is running or not"""
        process = self.startProcess(
            command=self.sudo,
            arguments=[self.docker_cmd, "ps"],
            abortOnError=False,
            stdouterr="docker_ps",
        )

        image_tag = self.generate_image_tag(image_name, image_version)
        self.assertGrep(
            process.stdout,
            f"{image_tag}",
            contains=not negate,
            abortOnError=abortOnError,
        )

    def assert_image_present(
        self, image_name, image_version=None, negate=False, abortOnError=False
    ):
        """Asserts that an image with the given image name and version is present on the system or not"""
        process = self.startProcess(
            command=self.sudo,
            arguments=[self.docker_cmd, "images"],
            abortOnError=False,
            stdouterr="docker_images",
        )

        image_regex = (
            image_name if image_version is None else f"{image_name}\s{image_version}"
        )
        self.assertGrep(
            process.stdout,
            f"{image_regex}",
            contains=not negate,
            abortOnError=abortOnError,
        )

    def docker_stop(self, container_id):
        """Use `docker stop` to stop a running container."""
        self.startProcess(
            command=self.sudo,
            arguments=[self.docker_cmd, "stop", container_id],
            abortOnError=False,
            stdouterr="docker_stop",
        )

    def docker_rm(self, container_id):
        """Use `docker stop` to remove a stopped container with its container id."""
        self.startProcess(
            command=self.sudo,
            arguments=[self.docker_cmd, "rm", container_id],
            abortOnError=False,
            stdouterr="docker_rm",
        )

    def docker_run(self, image_name, image_version=None):
        """Use `docker run` to run a container with the provided image name and version."""
        image_tag = self.generate_image_tag(image_name, image_version)
        process = self.startProcess(
            command=self.sudo,
            arguments=[self.docker_cmd, "run", "-d", image_tag],
            stdouterr="docker_run",
        )

        file = open(process.stdout, "r")
        container_id = file.readline()
        return container_id

    def docker_run_with_cleanup(self, image_name, image_version=None):
        container_id = self.docker_run(image_name, image_version)
        self.add_container_to_clean(container_id)
        self.add_image_to_clean(image_name, image_version)
        return container_id

    def docker_pull(self, image_name, image_version=None):
        """Use `docker pull` pull an image from docker registry."""
        image_tag = self.generate_image_tag(image_name, image_version)
        self.startProcess(
            command=self.sudo,
            arguments=[self.docker_cmd, "pull", image_tag],
            stdouterr="docker_pull",
        )

    def docker_pull_with_cleanup(self, image_name, image_version=None):
        """
        Use `docker pull` pull an image from docker registry
        with a cleanup function registered to cleanup the image after the test run
        """
        self.docker_pull(image_name, image_version)
        self.add_image_to_clean(image_name, image_version)

    def plugin_install(self, image_name, image_version=None):
        """Use docker plugin `install` command to install and run a container."""
        install_args = [image_name]
        if image_version is not None:
            install_args = install_args + ["--module-version", image_version]
        self.startProcess(
            command=self.sudo,
            arguments=[self.docker_plugin, "install"] + install_args,
            abortOnError=False,
            stdouterr="plugin_install",
        )
        return self.get_last_spawned_container_id()

    def plugin_install_with_cleanup(self, image_name, image_version=None):
        """
        TODO : If the plugin is updating containers self.containers_to_clean is not up to date anymore
        """
        container_id = self.plugin_install(image_name, image_version)
        self.add_container_to_clean(container_id)
        self.add_image_to_clean(image_name, image_version)

    def plugin_remove(self, image_name, image_version=None):
        """Use docker plugin `remove` command to stop and remove containers using the provided image name."""
        install_args = [image_name]
        if image_version is not None:
            install_args = install_args + ["--module-version", image_version]
        self.startProcess(
            command=self.sudo,
            arguments=[self.docker_plugin, "remove"] + install_args,
            abortOnError=False,
            stdouterr="plugin_remove",
        )
        return self.get_last_spawned_container_id()

    def plugin_finalize(self):
        """Use docker plugin `finalize` command to prune all unused images."""
        self.startProcess(
            command=self.sudo,
            arguments=[self.docker_plugin, "finalize"],
            abortOnError=False,
            stdouterr="plugin_finalize",
        )
        return self.get_last_spawned_container_id()

    def get_last_spawned_container_id(self):
        """Retrieve the container id of the last spawned container"""
        self.startProcess(
            command=self.sudo,
            arguments=[self.docker_cmd, "ps", "-q", "--latest"],
            abortOnError=False,
            stdouterr="docker_ps_latest",
        )

        file = open(self.output + "/docker_ps_latest.out", "r")
        container_id = file.readline().strip()
        return container_id

    def generate_image_tag(self, image_name, image_version=None):
        """Generate docker image tag from image name and version"""
        return image_name if image_version is None else f"{image_name}:{image_version}"

    def add_container_to_clean(self, container_id):
        self.containers_to_clean.append(container_id)

    def cleanup_containers(self):
        if len(self.containers_to_clean) > 0:
            self.log.info(
                f"Removing containers scheduled to be cleaned up: {self.containers_to_clean}"
            )
            self.startProcess(
                command=self.sudo,
                arguments=["docker", "rm", "-f"] + self.containers_to_clean,
                stdouterr="containers_for_cleanup",
                # TODO We ignore the exit status for now as it differs between docker version 20.10.5+dfsg1 and 18.09.1
                # There might me more containers listed as running as the plugin replaces containers
                ignoreExitStatus=True,
            )

    def add_image_to_clean(self, image_name, image_version=None):
        image_tag = self.generate_image_tag(image_name, image_version)
        self.images_to_clean.append(image_tag)

    def cleanup_images(self):
        if len(self.images_to_clean) > 0:
            self.log.info(
                f"Removing images scheduled to be cleaned up: {self.images_to_clean}"
            )
            self.startProcess(
                command=self.sudo,
                arguments=["docker", "rmi", "-f"] + self.images_to_clean,
                stdouterr="containers_for_cleanup",
            )
