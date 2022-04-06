import sys

from environment_docker_plugin import DockerPlugin


class DockerPluginFinalizeTest(DockerPlugin):

    image_name = "registry"

    def setup(self):
        super().setup()
        # Validate that an image with the given name is not present on the machine before test
        self.assert_image_present(self.image_name, negate=True, abortOnError=True)

        # Pull the test image that will be used for the test
        self.docker_pull_with_cleanup(self.image_name)
        self.assert_image_present(self.image_name, abortOnError=True)

    def execute(self):
        # This finalize call should cleanup the image that's pulled in the setup phase that's unused
        self.plugin_finalize()

    def validate(self):
        # Validate that the pulled image is not present as it should have been cleaned up by the finalize call
        self.assert_image_present(self.image_name, negate=True)
