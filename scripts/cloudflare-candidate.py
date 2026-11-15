#!/usr/bin/env python3
"""Separate signed preview-candidate policy. Never authorizes production traffic."""
import importlib.util
from pathlib import Path
import sys


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def policy():
    result = load('cloudflare-permit')
    result.SCHEMA = 2
    result.MODE = 'candidate-upload'
    result.REF = 'refs/heads/feat/cloudflare-candidate-preview'
    result.WORKFLOW = '.github/workflows/ci.yml'
    result.SIGNER = f'https://github.com/{result.REPOSITORY}/{result.WORKFLOW}@{result.REF}'
    result.DESTINATION = {'account_id': result.ACCOUNT, 'worker': 'animatix-candidate-preview'}
    result.RELEASE_PREFIX = 'cf-preview'
    return result


if __name__ == '__main__':
    if sys.argv[1:] == ['publish']:
        load('cloudflare-sdk-publisher').main(policy())
    elif sys.argv[1:] == ['build']:
        raise SystemExit(load('cloudflare-verified-consumer').main(policy(), candidate=True))
    else:
        raise SystemExit('usage: cloudflare-candidate.py publish|build')
