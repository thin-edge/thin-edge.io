import sys

from environment_docker_plugin import DockerPlugin

"""
Validate docker plugin install

Using `registry`[https://hub.docker.com/_/registry] as a test container

"""


class DockerPluginInstallTest(DockerPlugin):

    image_name = "registry"

    def setup(self):
        super().setup()
        # Assert that an image with the given name is not present on the machine before test
        self.assert_image_present(self.image_name, negate=True, abortOnError=True)

    def execute(self):
        self.plugin_install_with_cleanup(self.image_name)

    def validate(self):
        self.assert_container_running(self.image_name)
