"""Owner-selected Linux Actions runners; no automatic local execution admission."""
import os

SELF_HOSTED_NAMES = frozenset({'bin-actions', 'bin-actions2'})
HOSTED_LABELS = frozenset({'ubuntu-24.04', 'ubuntu-24.04-arm'})


def environment_allowed(env, *, self_hosted_only=False):
    if env.get('GITHUB_ACTIONS') != 'true':
        return False
    if env.get('RUNNER_ENVIRONMENT') == 'self-hosted':
        return env.get('RUNNER_NAME') in SELF_HOSTED_NAMES
    return (not self_hosted_only and env.get('RUNNER_ENVIRONMENT') == 'github-hosted'
            and env.get('RUNNER_OS') == 'Linux' and env.get('RUNNER_ARCH') in {'X64', 'ARM64'})


def require_environment(*, self_hosted_only=False, env=None):
    actual = os.environ if env is None else env
    if not environment_allowed(actual, self_hosted_only=self_hosted_only):
        raise ValueError('validation requires an owner-selected Linux Actions runner')
    return actual


def job_is_allowed(job, *, self_hosted_only=False):
    """GitHub jobs API evidence: registered names or exact standard hosted labels."""
    labels = set(job.get('labels') or [])
    if 'self-hosted' in labels:
        return (job.get('runner_name') in SELF_HOSTED_NAMES
                and {'self-hosted', 'Linux', 'X64'} <= labels)
    return not self_hosted_only and bool(labels & HOSTED_LABELS)


if __name__ == '__main__':
    require_environment()
