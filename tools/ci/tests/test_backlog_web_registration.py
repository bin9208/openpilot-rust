"""Guard the #252/#254/#270 acceptance registrations; run in Actions only."""
import ast
import os
from pathlib import Path
import re
import shlex
import unittest

import yaml


ROOT = Path(__file__).resolve().parents[3]
WORKFLOW = Path(os.environ.get('BACKLOG_WEB_WORKFLOW', ROOT / '.github/workflows/backlog-fixes-497.yml'))


def required_options(script):
    """Read the real argparse declarations without importing any project probe."""
    tree = ast.parse((ROOT / 'rust/tools' / script).read_text())
    options = set()
    for node in ast.walk(tree):
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute) and node.func.attr == 'add_argument':
            if any(key.arg == 'required' and isinstance(key.value, ast.Constant) and key.value.value is True for key in node.keywords):
                if node.args and isinstance(node.args[0], ast.Constant):
                    options.add(node.args[0].value)
        if isinstance(node, ast.For) and isinstance(node.iter, (ast.List, ast.Tuple)):
            calls = [child for child in ast.walk(node) if isinstance(child, ast.Call)
                     and isinstance(child.func, ast.Attribute) and child.func.attr == 'add_argument'
                     and any(key.arg == 'required' and isinstance(key.value, ast.Constant) and key.value.value is True for key in child.keywords)]
            if calls and all(isinstance(item, ast.Constant) and isinstance(item.value, str) for item in node.iter.elts):
                options.update('--' + item.value for item in node.iter.elts)
    return options


class BacklogWebRegistrationTests(unittest.TestCase):
    def setUp(self):
        self.job = yaml.load(WORKFLOW.read_text(), Loader=yaml.BaseLoader)['jobs']['focused']
        self.steps = self.job['steps']
        self.web = [step for step in self.steps if step.get('if') == "matrix.scope == 'web'"]
        self.script = '\n'.join(step.get('run', '') for step in self.web)

    def invocations(self, script):
        lines = [line.strip() for line in self.script.splitlines()
                 if re.search(r'\s-P\s+.*?/' + re.escape(script) + r'[" ]', line)]
        self.assertTrue(lines, script)
        arrays = {name: shlex.split(body) for name, body in re.findall(r'^\s*(\w+)=\(([^\n]*)\)\s*$', self.script, re.M)}
        calls = []
        for line in lines:
            words = shlex.split(line)
            expanded = []
            for word in words:
                match = re.fullmatch(r'\$\{(\w+)\[@\]\}', word)
                expanded.extend(arrays[match[1]] if match else [word])
            self.assertTrue(required_options(script).issubset(expanded), (script, expanded))
            self.assertNotIn('--start-case', expanded)
            self.assertNotIn('--reuse-codec', expanded)
            self.assertNotIn('--only-zstd', expanded)
            self.assertNotIn('--native-only', expanded)
            calls.append(expanded)
        return calls

    def test_full_report_capture_chain_feeds_the_unfiltered_native_comparison(self):
        names = [f'carrot_server_dashcam_report_{part}.py' for part in ('source_controls', 'state_controls', 'open_stream')]
        for name in names:
            self.invocations(name)
        comparison = self.invocations('carrot_server_dashcam_report.py')[0]
        for option, suffix in (('--source-captures', 'report/source'), ('--state-captures', 'report/state'),
                               ('--open-stream-captures', 'report/open-stream')):
            self.assertTrue(comparison[comparison.index(option) + 1].endswith('/' + suffix))
        self.assertLess(self.script.index(names[-1]), self.script.index('carrot_server_dashcam_report.py'))

    def test_both_live_probes_run_with_real_bindings_and_node(self):
        for name in ('carrot_server_live.py', 'carrot_server_live_app.py'):
            self.invocations(name)
        node = next(step for step in self.web if step.get('uses', '').startswith('actions/setup-node@'))
        self.assertGreaterEqual(int(node['with']['node-version']), 20)
        for helper in ('build_params_python.py', 'build_visionipc_python.py'):
            self.assertIn(helper, self.script)
        self.assertIn('ORIGINAL_PARAMS_BINDING=', self.script)
        self.assertIn('msgq.ipc_pyx', self.script)
        self.assertIn('msgq.visionipc.visionipc_pyx', self.script)

    def test_terminal_original_native_edges_and_all_application_lifecycle_lanes(self):
        edges = self.invocations('carrot_server_terminal_edges.py')
        self.assertEqual(len(edges), 2)
        self.assertEqual(sum('--native' in call and '--launcher' in call for call in edges), 1)
        self.assertIn('for mode in reset eof; do', self.script)
        self.assertEqual(len(self.invocations('carrot_server_terminal_cli.py')), 1)
        self.invocations('carrot_server_terminal_cli_errors.py')
        http = self.invocations('carrot_server_terminal_http.py')
        self.assertEqual(len(http), 2)
        self.assertEqual(sum('--application' in call for call in http), 1)
        pty = self.invocations('carrot_server_terminal_pty.py')
        self.assertEqual(len(pty), 3)
        self.assertEqual(sum('--native' not in call for call in pty), 1)
        self.assertEqual(sum('--native' in call and '--filter' in call for call in pty), 1)
        self.invocations('carrot_server_terminal_limits.py')
        self.assertIn('for mode in history shutdown missing params; do', self.script)
        captures = self.invocations('carrot_server_terminal_capture.py')
        self.assertEqual(len(captures), 4)
        self.assertEqual(sum('--timeout' in call for call in captures), 1)
        self.assertEqual(sum('--force' in call for call in captures), 1)
        self.invocations('carrot_server_main_control.py')
        self.assertIn('for mode in term supervisor force; do', self.script)

    def test_builds_and_private_source_view_precede_probes_and_upload_precedes_cleanup(self):
        for example in ('dashcam_report', 'carrot_live', 'carrot_terminal', 'carrot_terminal_command', 'carrot_main_owner'):
            self.assertIn('--example ' + example, self.script)
        self.assertIn('-p openpilot-carrot-server -p openpilot-process-supervision --bins', self.script)
        self.assertEqual(self.job['runs-on'], ['self-hosted', 'Linux', 'X64', 'dev-md-local'])
        self.assertIn('github.event.pull_request.head.repo.full_name == github.repository', self.job['if'])
        self.assertIn('ln -s "$GITHUB_WORKSPACE/openpilot" "$RUNNER_TEMP/web-source/openpilot"', self.script)
        stages = [step for step in self.web if 'carrot_server_' in step.get('run', '')]
        self.assertTrue(all(step.get('working-directory') == '${{ env.CI_JOB_TEMP }}/web-source' for step in stages))
        self.assertIn('unshare --mount --net', self.script)
        self.assertIn('mount --make-rprivate /', self.script)
        self.assertIn('chroot "$root"', self.script)
        self.assertIn('GIT_CONFIG_VALUE_0=/data/openpilot', self.script)
        self.assertLess(self.script.index('archive HEAD:openpilot/selfdrive/carrot/web'), self.script.index('sudo -n unshare'))
        uploads = [index for index, step in enumerate(self.steps) if step.get('uses', '').startswith('actions/upload-artifact@')]
        self.assertTrue(uploads)
        upload = self.steps[uploads[-1]]
        self.assertEqual(upload['if'], 'always()')
        self.assertIn('${{ env.CI_JOB_TEMP }}/web-source/evidence/', upload['with']['path'])
        self.assertEqual(self.steps[-1].get('uses'), './.github/actions/job-temp')
        self.assertEqual(self.steps[-1]['with']['cleanup'], 'true')
        self.assertLess(max(self.steps.index(step) for step in stages), uploads[-1])


if __name__ == '__main__':
    unittest.main()
