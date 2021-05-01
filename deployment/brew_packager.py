"""Generate a package using information retrieved from env vars

This is meant to be used within a CI to generate Homebrew package information 
for releases.

Usage:

```sh
package.py TEMPLATE_FILE OUTPUT_FILE RELEASE_VERSION CHECKSUM
```

Arguments:

* `TEMPLATE_FILE`: The input file to fill in
* `OUTPUT_FILE`: The path to where the substituted template file should be 
  written
* `RELEASE_VERSION`: The string corresponding to the release version of the 
  package
* `CHECKSUM`: The SHA256 checksum of the package
"""

from string import Template
from pathlib import Path
from typing import Dict
import sys


def sub_metadata(template_file: Path, package_meta: Dict[str, str]) -> str:
    """Replace template arguments in a document with the actual information
    pulled from the environment.

    Args:
        template_file: The path to the template file
        package_meta: A map with package metadata to use when performing the
          file substitution

    Returns:
        The string contents of the template file with replacements for the
        metadata variables.
    """

    with open(template_file) as f:
        template = Template(f.read())

    return template.safe_substitute(
        version=package_meta["version"],
        shachecksum=package_meta["checksum"],
    )


def main():
    input_file = sys.argv[1]
    output_file = sys.argv[2]
    release_version = sys.argv[3]
    checksum = sys.argv[4]

    print(f"Generating {output_file} from template {input_file}")

    metadata = {
        "version": release_version,
        "checksum": checksum,
    }

    print("Metadata:")

    for k, v in metadata.items():
        print(f"* {k}: {v}")

    output_contents = sub_metadata(Path(input_file), metadata)

    with open(output_file, "w") as out_fd:
        out_fd.write(output_contents)
        print(f"Generated {output_file}")


if __name__ == "__main__":
    main()
