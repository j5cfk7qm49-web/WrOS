"""CI workflow checks for publishing the NixOS channel and static Pages content."""
import importlib.util
import os
from pathlib import Path
import re
import subprocess
import tempfile
import textwrap
import unittest

ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = (ROOT / '.github/workflows/release-updates.yml').read_text()


class Workflow(unittest.TestCase):
    def test_triggers_and_expiry_check_are_fixed(self):
        for needle in ("tags: ['v*', '!v*-linktest*']", 'schedule:', "cron: '17 6 * * 1'",
                       'workflow_dispatch:', 'nixos_release:', 'actions/deploy-pages'):
            self.assertIn(needle, WORKFLOW)
        self.assertEqual(set(re.findall(r'secrets\.(\w+)', WORKFLOW)), {'GITHUB_TOKEN'})
        # The run's own token is used once: the scheduled channel-expiry check opens an issue with it.
        self.assertEqual(WORKFLOW.count('secrets.GITHUB_TOKEN'), 1)
        expiry = WORKFLOW.split('- name: Check the published NixOS channel expiry', 1)[1].split('\n      - ', 1)[0]
        self.assertIn('GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}', expiry)
        self.assertIn("if: github.event_name == 'schedule'", expiry)
        self.assertIn('https://j5cfk7qm49-web.github.io/WrOS/nixos-updates/InRelease', expiry)
        self.assertIn('tools/publish-nixos-updates.py days-left out/InRelease --with-date', expiry)
        self.assertIn('[ "$days_left" -ge 10 ]', expiry)
        self.assertIn('gh issue list', expiry)
        self.assertIn('gh issue create', expiry)
        self.assertEqual(WORKFLOW.count("if: steps.plan.outputs.deploy == 'true'"), 3)
        # Pages serves exactly what this run built: a folder the repository does not have.
        served = re.search(r'upload-pages-artifact@v\d+\n.*\n\s+with:\n\s+path: (\S+)', WORKFLOW).group(1)
        self.assertEqual(served, 'out/pages')
        self.assertFalse((ROOT / served).exists())
        self.assertNotIn(' site', WORKFLOW.replace('the whole site', ''))

    def test_every_deploy_carries_the_static_pages_beside_the_updates(self):
        # Each Pages deploy replaces the whole site, so the linking and privacy pages ride along.
        step = WORKFLOW.split('- name: Static pages', 1)[1].split('\n      - ', 1)[0]
        self.assertIn("if: steps.plan.outputs.deploy == 'true'", step)
        self.assertIn('cp -r pages/. out/pages/', step)
        self.assertLess(WORKFLOW.index('- name: Static pages'), WORKFLOW.index('upload-pages-artifact'))
        pages = sorted(p.relative_to(ROOT / 'pages').as_posix() for p in (ROOT / 'pages').rglob('*') if p.is_file())
        self.assertEqual(pages, ['index.html', 'l/index.html', 'link/index.html', 'privacy/index.html'])
        privacy = (ROOT / 'pages/privacy/index.html').read_text()
        self.assertIn('passes through', privacy)
        self.assertIn('useless without', privacy)


TREE = (ROOT / 'os/VERSION').read_text().splitlines()[0].strip()
TAG = 'v' + TREE.replace('~', '-')


def workflow_script(name):
    block = WORKFLOW.split('- name: ' + name, 1)[1].split('\n      - ', 1)[0]
    return textwrap.dedent(block.split('run: |\n', 1)[1])


def plan(event, *, ref='', nixos=''):
    """Run the workflow's 'Plan this run' step as GitHub would, returning its outputs."""
    script = workflow_script('Plan this run')
    with tempfile.TemporaryDirectory(prefix='wros-plan-') as tmp:
        output = Path(tmp) / 'output'
        output.touch()
        env = {'PATH': os.environ['PATH'], 'GITHUB_EVENT_NAME': event, 'GITHUB_REF_NAME': ref,
               'GITHUB_OUTPUT': str(output), 'NIXOS': nixos}
        result = subprocess.run(['bash', '-c', script], cwd=ROOT, env=env, capture_output=True, text=True)
        if result.returncode != 0:
            raise ValueError(result.stderr)
        return dict(line.split('=', 1) for line in output.read_text().splitlines())


class Plan(unittest.TestCase):
    def test_every_deploy_names_a_nixos_release(self):
        cases = [
            (('push', dict(ref=TAG)), (TAG, 'true')),
            (('workflow_dispatch', dict(nixos=TAG)), (TAG, 'true')),
            # Naming no NixOS release deploys nothing: Pages would lose nixos-updates/.
            (('schedule', {}), ('', 'false')),
            (('workflow_dispatch', {}), ('', 'false')),
        ]
        for (event, options), expected in cases:
            with self.subTest(event=event, **options):
                out = plan(event, **options)
                self.assertEqual((out['nixos'], out['deploy']), expected)
                self.assertEqual(out['nixos_version'], TREE if expected[0] else '')
        # A release must name the version in the tree it is published from.
        with self.assertRaisesRegex(ValueError, 'os/VERSION'):
            plan('workflow_dispatch', nixos='v9.9.9')
        with self.assertRaisesRegex(ValueError, 'os/VERSION'):
            plan('push', ref='v9.9.9')

    def test_a_deploy_is_all_or_nothing(self):
        step = WORKFLOW.split('- name: Static pages', 1)[1].split('\n      - ', 1)[0]
        required = re.search(r'^\s*for part in (.+); do$', step, re.M).group(1).split()
        for part in ('index.html', 'nixos-updates/InRelease', 'nixos-updates/closure-manifest.json',
                     'l/index.html'):
            self.assertIn(part, required)
        nixos = WORKFLOW.split("- name: Serve the NixOS release's offline-signed channel", 1)[1]
        nixos = nixos.split('\n      - ', 1)[0]
        self.assertIn("if: steps.plan.outputs.nixos != ''", nixos)
        self.assertIn('releases/download/$NIXOS"', nixos)
        self.assertIn('"$base/wros-nixos-updates-thin.tar"', nixos)
        self.assertIn('"$base/wros-nixos-updates.tar"', nixos)
        self.assertLess(nixos.index('wros-nixos-updates-thin.tar'), nixos.index('"$base/wros-nixos-updates.tar"'))
        self.assertIn('tools/publish-nixos-updates.py check-bundle', nixos)
        self.assertIn('-size +99M', nixos)
        self.assertLess(WORKFLOW.index("Serve the NixOS release"), WORKFLOW.index('- name: Static pages'))
        self.assertFalse((ROOT / '.github/workflows/release-nixos-updates.yml').exists(),
                         'a second Pages workflow would replace this one\'s deploys')


class TagVersion(unittest.TestCase):
    def test_a_tag_must_name_the_version_in_the_tree(self):
        spec = importlib.util.spec_from_file_location('publish_nixos_updates', ROOT / 'tools/publish-nixos-updates.py')
        publish = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(publish)
        self.assertEqual(publish.tag_version('v0.1.0-rc3', '0.1.0~rc3'), '0.1.0~rc3')
        self.assertEqual(publish.tag_version('v0.2.0', '0.2.0'), '0.2.0')
        for tag, tree in (('v0.1.0-rc3', '0.1.0~rc2'), ('0.1.0-rc3', '0.1.0~rc3'), ('v0.1.0-rc3;x', '0.1.0~rc3')):
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                publish.tag_version(tag, tree)


if __name__ == '__main__':
    unittest.main()
