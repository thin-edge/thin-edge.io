import sys

from environment_docker_plugin import DockerPlugin

"""
Validate docker plugin install

Using `registry`[https://hub.docker.com/_/registry] as a test container

"""


class DockerPluginRemoveTest(DockerPlugin):

    image_name = "registry"

    def setup(self):
        super().setup()
        # Assert that an image with the given name is not present on the machine before test
        self.assert_image_present(
            self.image_name, negate=True, abortOnError=True)

        # Run a container with the test image that is to be removed during the test
        self.docker_run(self.image_name)
        self.assert_container_running(self.image_name)

    def execute(self):
        # Stop and remove all containers using the test image with the plugin remove command
        self.plugin_remove(self.image_name)

    def validate(self):
        # Assert that no containers using the given image name and version are running after the plugin remove call
        self.assert_container_running(self.image_name, negate=True)
