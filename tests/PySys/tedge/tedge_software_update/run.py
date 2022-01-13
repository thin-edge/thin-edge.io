from .environments.environment_c8y import EnvironmentC8y

"""
Validate command line option help

Given a running system
When we call tedge help
Then we find the string USAGE: in the output
Then we find the string FLAGS: in the output
Then we find the string SUBCOMMANDS: in the output
"""

TEDGE_DOWNLOAD_DIR = "/tedge_download_dir"
TEDGE_DOWNLOAD_PATH = "tmp.path"
TOPIC = 'tedge/commands/req/software/update'
PAYLOAD = '{"id":"1234","updateList":[{"type":"apt","modules":[{"name":"rolldice","version":"::apt","url":"https://t48415.basic.stage.c8y.io/inventory/binaries/1202","action":"install"}]}]}'


class PySysTest(EnvironmentC8y):

    sudo = "/usr/bin/sudo"
    tedge = "/usr/bin/tedge"

    def tedge_get_config(self, filename: str):
        """
        run tedge config get `TEDGE_DOWNLOAD_PATH`

        this is used in validation
        """
        _ = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "config", "get", TEDGE_DOWNLOAD_PATH],
            stdouterr=filename,
            expectedExitStatus="==0",
        )

    def tedge_set_config(self, new_value: str):
        """
        run tedge config set `TEDGE_DOWNLOAD_PATH` `new_value`
        """
        _ = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "config", "set", TEDGE_DOWNLOAD_PATH, new_value],
            expectedExitStatus="==0",
        )

    def setup(self):
        super().setup()
        self.log.info("Setup")
        self.addCleanupFunction(self.mycleanup)

    def execute(self):
        """
        1. saving existing download path
        2. setting download path to `TEDGE_DOWNLOAD_DIR`
        3. querying new download path (for validation)
        4. running software update
        """

        # make a new directory `TEDGE_DOWNLOAD_DIR`
        _ = self.startProcess(
            command=self.sudo,
            arguments=["mkdir", TEDGE_DOWNLOAD_DIR]
        )

        # give full permission to `TEDGE_DOWNLOAD_DIR`
        _ = self.startProcess(
            command=self.sudo,
            arguments=["chmod", "a+rwx", TEDGE_DOWNLOAD_DIR]
        )

        # 1. save the current/pre-change setting in /Output
        self.tedge_get_config(filename="tedge_config_get_original")

        # 2. change tedge download path to `TEDGE_DOWNLOAD_DIR`
        self.tedge_set_config(new_value=TEDGE_DOWNLOAD_DIR)

        # 3. tedge config get on changed value
        self.tedge_get_config(filename="tedge_config_get_new_value")

        # NOTE: remove `rolldice` if already there
        # 4. trigger rolldice download
        _ = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "mqtt", "pub", TOPIC, PAYLOAD],
            stdouterr="rolldice_download",
            expectedExitStatus="==0",
        )

    def validate(self):
        self.assertGrep("tedge_config_get_new_value.out", f'{TEDGE_DOWNLOAD_DIR}', contains=True)

    def cleanup(self):

        with open("Output/linux/tedge_config_get_original.out", "r") as handle:
            original_value = handle.read().strip()

        # reverting to original value
        self.tedge_set_config(new_value=original_value)

        # querying value
        self.tedge_get_config(filename="tedge_config_get_cleanup")

        # asserting it is the same as `original_value`
        self.assertGrep("tedge_config_get_cleanup.out", f'{original_value}', contains=True)

        # removing tedge dir
        _ = self.startProcess(
            command=self.sudo,
            arguments=["rmdir", TEDGE_DOWNLOAD_DIR]
        )

        return super().cleanup()
