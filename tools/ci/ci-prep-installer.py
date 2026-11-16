#!/usr/bin/env python3
"""Download the reviewed official installer without reading/writing Actions cache."""
import hashlib
import os
from pathlib import Path
import urllib.request

URL = 'https://cf.install.determinate.systems/nix-installer/tag/v3.24.2/x86_64-linux'
SHA256 = '22ed14ee1b24d100d388f1fc245e3062bd5982433d159cffa6dec34c0eb15dab'
SIZE = 78449072


def main():
    root = Path(os.environ['RUNNER_TEMP'])
    destination = root / 'ci-prep-nix-installer'
    digest = hashlib.sha256()
    size = 0
    # Fresh runner-owned path; never overwrite a preexisting executable.
    with urllib.request.urlopen(URL, timeout=90) as response, destination.open('xb') as output:
        while chunk := response.read(1024 * 1024):
            size += len(chunk)
            if size > SIZE:
                raise ValueError('unexpected installer size')
            digest.update(chunk)
            output.write(chunk)
    if size != SIZE or digest.hexdigest() != SHA256:
        raise ValueError('official installer checksum mismatch')
    destination.chmod(0o700)
    print('Verified official nix-installer v3.24.2; no Actions cache used')


if __name__ == '__main__':
    main()
