#!/usr/bin/env python3
"""Shared Actions admission for registered private and native hosted runners."""
import argparse
from collections.abc import Mapping
import os

SELF_HOSTED_NAMES = frozenset(('bin-actions', 'bin-actions2'))
HOSTED_LABELS = frozenset(('ubuntu-24.04', 'ubuntu-24.04-arm'))


def environment_allowed(env, *, self_hosted_only=False):
    if not isinstance(env, Mapping) or env.get('GITHUB_ACTIONS') != 'true':
        return False
    if env.get('RUNNER_ENVIRONMENT') == 'self-hosted':
        name = env.get('RUNNER_NAME')
        return isinstance(name, str) and name in SELF_HOSTED_NAMES
    return (not self_hosted_only and env.get('RUNNER_ENVIRONMENT') == 'github-hosted'
            and env.get('RUNNER_OS') == 'Linux' and env.get('RUNNER_ARCH') in ('X64', 'ARM64'))


def require_environment(*, self_hosted_only=False, env=None):
    if not environment_allowed(os.environ if env is None else env, self_hosted_only=self_hosted_only):
        raise ValueError('registered self-hosted Actions runner required' if self_hosted_only
                         else 'registered self-hosted or supported Linux GitHub-hosted Actions runner required')


def job_is_allowed(job, *, self_hosted_only=False):
    if not isinstance(job, Mapping):
        return False
    labels = job.get('labels')
    if not isinstance(labels, (list, tuple)) or not all(isinstance(label, str) for label in labels):
        return False
    labels = set(labels)
    name = job.get('runner_name')
    if 'self-hosted' in labels:
        return isinstance(name, str) and name in SELF_HOSTED_NAMES and {'self-hosted', 'Linux', 'X64'} <= labels
    return (not self_hosted_only and bool(labels & HOSTED_LABELS)
            and isinstance(name, str) and bool(name.strip()))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--self-hosted-only', action='store_true')
    arguments = parser.parse_args()
    require_environment(self_hosted_only=arguments.self_hosted_only)
