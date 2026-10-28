"""Wi-Fi packaging contracts with a synthetic dpkg database, never a radio."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('wros_wifi_audit', ROOT/'tools/audit-wifi-image.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class WifiImagePolicy(unittest.TestCase):
    def test_image_recipe_carries_common_laptop_firmware_and_maintains_repository_component(self):
        required = {'firmware-iwlwifi', 'firmware-realtek', 'firmware-atheros',
                    'firmware-ath9k-htc', 'firmware-brcm80211', 'firmware-mediatek',
                    'network-manager', 'wpasupplicant', 'wireless-regdb'}
        recipe = set((ROOT/'os/config/packages.txt').read_text().splitlines())
        self.assertTrue(required <= recipe)
        policy = json.loads((ROOT/'os/config/wifi-firmware.json').read_text())
        self.assertTrue(policy['offline_media_required'])
        self.assertEqual(policy['physical_adapters_verified'], [])
        script = (ROOT/'os/scripts/bootstrap-rootfs.sh').read_text()
        self.assertEqual(script.count('main non-free-firmware'), 3)

    def fixture(self, root):
        policy = json.loads((ROOT/'os/config/wifi-firmware.json').read_text())
        names = [policy['kernel_package'], *policy['connection_packages'],
                 *(p['package'] for p in policy['firmware'])]
        info = root/'var/lib/dpkg/info'
        info.mkdir(parents=True)
        (info.parent/'status').write_text('\n\n'.join(
            f'Package: {name}\nStatus: install ok installed\nArchitecture: all\nVersion: 1\nMaintainer: Fixture <fixture@example.invalid>\nDescription: synthetic audit fixture\n'
            for name in names))
        for item in policy['firmware']:
            name = item['package']
            payload = root/'usr/lib/firmware'/(name+'.fixture')
            payload.parent.mkdir(parents=True, exist_ok=True)
            payload.write_bytes(b'synthetic test payload, not actual firmware')
            (info/(name+'.list')).write_text('/'+payload.relative_to(root).as_posix()+'\n')
            notice = root/'usr/share/doc'/name/'copyright'
            notice.parent.mkdir(parents=True)
            notice.write_text('synthetic test notice')
        (root/'usr/bin').mkdir(parents=True)
        (root/'usr/bin/nmtui').write_text('synthetic test executable')
        (root/'usr/lib/modules/fixture/kernel/drivers/net/wireless').mkdir(parents=True)

    def test_real_dpkg_query_reads_staged_inventory_and_requires_payload_not_only_package_names(self):
        with tempfile.TemporaryDirectory(prefix='wros-wifi-audit-') as tmp:
            root = Path(tmp)
            self.fixture(root)
            result = module.audit(root)
            self.assertEqual(len(result['firmware_payload']), 6)
            self.assertEqual(result['physical_adapters_verified'], [])
            (root/'usr/lib/firmware/firmware-iwlwifi.fixture').unlink()
            with self.assertRaisesRegex(ValueError, 'no installed firmware'):
                module.audit(root)

    def test_missing_connection_interface_and_live_root_are_refused(self):
        with self.assertRaisesRegex(ValueError, 'never the live root'):
            module.audit(Path('/'))
        with tempfile.TemporaryDirectory(prefix='wros-wifi-missing-') as tmp:
            root = Path(tmp)
            self.fixture(root)
            (root/'usr/bin/nmtui').unlink()
            with self.assertRaisesRegex(ValueError, 'nmtui is missing'):
                module.audit(root)
