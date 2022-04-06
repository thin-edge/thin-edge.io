import sys

from environment_docker_plugin import DockerPlugin

"""
Validate docker plugin install

Using `registry`[https://hub.docker.com/_/registry] as a test container

"""


class DockerPluginUpdateTest(DockerPlugin):

    image_name = "registry"
    initial_image_version = "2.6.2"
    final_image_version = "2.7.1"

    def setup(self):
        super().setup()
        # Assert that an image with the given name is not present on the machine before test
        self.assert_image_present(self.image_name, negate=True, abortOnError=True)

        # Run a container with the test image with an old version that is to be updated during the test
        self.docker_run_with_cleanup(self.image_name, self.initial_image_version)
        self.assert_container_running(self.image_name, self.initial_image_version)

    def execute(self):
        # Update the container using the initial image version with the final image version with the plugin install command
        self.plugin_install_with_cleanup(self.image_name, self.final_image_version)

    def validate(self):
        # Assert that no containers using the initial image version are running after the update with plugin install command
        self.assert_container_running(
            self.image_name, self.initial_image_version, negate=True
        )

        # Assert that new containers using the final image version are running after the update with the plugin install call
        self.assert_container_running(self.image_name, self.final_image_version)
