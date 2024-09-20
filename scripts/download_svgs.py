import os
import requests
from requests.auth import HTTPBasicAuth
from dotenv import load_dotenv

# Load environment variables from .env file
load_dotenv()

# Get Munin credentials from environment variables
MUNIN_USERNAME = os.getenv('MUNIN_USERNAME')
MUNIN_PASSWORD = os.getenv('MUNIN_PASSWORD')

# Check if the credentials are available
if not MUNIN_USERNAME or not MUNIN_PASSWORD:
    raise EnvironmentError("Munin credentials are not set in the environment variables.")

# List of SVG filenames to download
svg_files = ['tedgecpuprocent-month.svg', 'tedgemem-month.svg']

# URL of the Munin server
base_url = 'https://munin.osadl.org/munin/osadl.org/rackfslot1.osadl.org/'

# Set the output folder relative to the project folder
project_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
output_folder = os.path.join(project_dir, 'docs', 'src', 'benchmarks')

# Download each SVG file
for svg_file in svg_files:
    svg_url = f"{base_url}{svg_file}"
    output_path = os.path.join(output_folder, svg_file)

    try:
        # Request the SVG file with basic authentication
        response = requests.get(svg_url, auth=HTTPBasicAuth(MUNIN_USERNAME, MUNIN_PASSWORD), verify=False)
        response.raise_for_status()

        # Write the SVG content to a file
        with open(output_path, 'wb') as f:
            f.write(response.content)

        print(f"Downloaded {svg_file} to {output_path}")

    except requests.exceptions.RequestException as e:
        print(f"Failed to download {svg_file}: {e}")
