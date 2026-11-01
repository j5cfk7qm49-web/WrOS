"""Allowlisted answers for the native WrOS installer forms (RC production-path VM run).

Pure logic, no VM access: `parse_form` turns the text of /dev/vcs7 into a Form; `decide`
returns the Action for a Form from an explicit allowlist, or None for an unknown form.
The driver never answers a form for which decide() is None.
"""
import re
from dataclasses import dataclass, field

MARK = '▸ '  # the list highlight shared by the installer and Settings


@dataclass
class Form:
    status: str = ''           # "<title> . N of M"
    description: str = ''      # accent-coloured prompt text
    body: str = ''             # everything between header and the input rows
    selected: str = ''         # text of the highlighted row / editable answer
    rows: list = field(default_factory=list)
    raw: str = ''
    kind: str = 'unknown'      # select | boolean | string | password | note | progress | blank

    def has(self, pattern):
        return re.search(pattern, self.raw, re.I | re.S) is not None


def parse_form(lines):
    cleaned = []
    for line in lines:
        # panel borders and padding
        stripped = line.strip().strip('│┃|').strip()
        if stripped[:1] in ('┌', '└', '├'):
            continue
        cleaned.append(stripped + ' ' if stripped == MARK.strip() else stripped)
    raw = '\n'.join(l for l in cleaned if l)
    form = Form(raw=raw)
    if not raw.strip():
        form.kind = 'blank'
        return form
    marked = [i for i, l in enumerate(cleaned) if l.startswith(MARK) or (MARK in l and l.index(MARK) < 4)]
    for l in cleaned:
        if re.search(r'\bof\s+\d+\b', l) and '·' in l:
            form.status = l
            break
    if not marked:
        form.kind = 'progress' if re.search(r'\d+\s*%|Preparing|installing|Configuring|Running|Retrieving|Detecting|Setting up|Loading|Scanning|Partitioning|Formatting|Copying|Extracting', raw, re.I) else 'unknown'
        form.description = raw
        return form
    first = marked[0]
    form.selected = cleaned[first].split(MARK, 1)[1].strip() if MARK in cleaned[first] else cleaned[first]
    head = [l for l in cleaned[:first] if l]
    # drop title bar and status lines (the first two non-empty lines are the panel chrome)
    form.description = '\n'.join(head)
    form.body = form.description
    rows = []
    body = [l for l in cleaned[first:] if l]
    help_at = [i for i, l in enumerate(body) if re.search(r'↑↓|Enter continue|^F1 help', l)]
    if help_at and help_at[0] > 0 and not body[help_at[0] - 1].startswith(MARK):
        del body[help_at[0] - 1]      # the title line printed just above the key-help footer
    for l in body:
        if re.search(r'↑↓|Enter continue|F1 help|PgUp', l):
            continue
        rows.append(l.split(MARK, 1)[1].strip() if MARK in l else l)
    form.rows = rows
    if form.selected == '[hidden]':
        form.kind = 'password'
    elif rows[:2] == ['No', 'Yes'] or (form.selected in ('No', 'Yes') and set(rows[:2]) <= {'No', 'Yes'}):
        form.kind = 'boolean'
    elif form.selected == 'Enter to continue':
        form.kind = 'note'
    elif len(rows) > 1 or re.match(r'^\[[ x]\]', form.selected):
        form.kind = 'select'
    else:
        form.kind = 'string'
    return form


# Each action: ('enter',), ('yes',), ('no',), ('select', regex_to_choose, regex_to_avoid),
# ('fill', text), ('secret', name), ('wait',)
@dataclass
class Rule:
    name: str
    pattern: str
    action: tuple
    kinds: tuple = ()
    requires: str = ''      # extra regex that must also match (e.g. target identity)

    def matches(self, form):
        # a list whose marked row is the last (or only) row has no rows after it and parses as 'string'
        kinds = self.kinds + ('string',) if 'select' in self.kinds and 'string' not in self.kinds else self.kinds
        if kinds and form.kind not in kinds:
            return False
        if not re.search(self.pattern, form.raw, re.I | re.S):
            return False
        return not self.requires or re.search(self.requires, form.raw, re.I | re.S) is not None


def rules(layout_choice=r'Colemak(?![-\w])', avoid=r'DH|Dvorak|Workman|Norman'):
    """The complete allowlist. Keep strict; anything else stops the run."""
    return [
        Rule('language', r'Select a language', ('enter',), ('select',)),
        Rule('location', r'Select your location', ('enter',), ('select',)),
        Rule('keyboard-country', r'Country of origin for the keyboard', ('select', r'^English \(US\)$', ''), ('select',)),
        Rule('keyboard-layout', r'Keyboard layout|Keymap to use|keyboard variant', ('select', layout_choice, avoid), ('select',)),
        Rule('hostname', r'Hostname', ('fill', 'wros-rc'), ('string',)),
        Rule('domain', r'Domain name', ('fill', 'invalid'), ('string',)),
        Rule('fullname', r'Full name for the new user', ('fill', 'RC Writer'), ('string',)),
        Rule('username', r'Username for your account', ('fill', 'writer'), ('string',)),
        Rule('user-password-1', r'Choose a password for the new user', ('secret', 'login'), ('password',)),
        Rule('user-password-2', r'Re-enter password to verify', ('secret', 'login'), ('password',)),
        Rule('weak-password', r'(weak|short|dictionary).{0,60}password|Use weak password', ('wait',), ('boolean',)),
        Rule('timezone', r'Select your time zone', ('enter',), ('select',)),
        Rule('partition-method', r'Partitioning method', ('select', r'Guided - use entire disk and set up encrypted LVM', ''), ('select',)),
        Rule('select-disk', r'Select disk to partition', ('select', r'vda|virtio', ''), ('select', 'string')),   # a single-disk list renders as one marked row
        Rule('partition-scheme', r'Partitioning scheme', ('enter',), ('select',)),
        Rule('erase-no', r'(Erase data|erase).{0,80}(disk|vda)', ('no',), ('boolean',)),
        Rule('luks-1', r'Encryption passphrase', ('secret', 'luks'), ('password',)),
        Rule('luks-2', r'Re-enter passphrase to verify', ('secret', 'luks'), ('password',)),
        Rule('lvm-size', r'Amount of volume group to use for guided partitioning', ('enter',), ('string',)),
        Rule('write-lvm', r'Write the changes to disks and configure LVM\?', ('yes',), ('boolean',), r'vda|LVM'),
        Rule('write-crypt', r'Write the changes to disk and configure encrypted volumes\?', ('yes',), ('boolean',), r'vda|encrypted'),
        Rule('new-label', r'Create new empty partition table on this device\?', ('yes',), ('boolean',), r'vda'),
        Rule('partition-finish', r'overview of your currently configured partitions',
             ('select', r'Finish partitioning and write changes to disk', ''), ('select',), r'vda'),
        Rule('write-disks', r'Write the changes to disks\?', ('yes',), ('boolean',), r'vda|LVM VG debian-vg'),  # list window can scroll the vda line off; vda is the only attached disk
        Rule('mirror', r'Use a network mirror\?', ('no',), ('boolean',)),
        Rule('grub-only-os', r'Install the GRUB boot loader to your primary drive\?', ('yes',), ('boolean',), r'only operating system'),
        Rule('grub-nvram', r'Update NVRAM variables to automatically boot into Debian\?', ('yes',), ('boolean',)),
        Rule('grub-removable', r'Force GRUB installation to the EFI removable media path\?', ('no',), ('boolean',)),
        Rule('no-interfaces', r'No network interfaces detected', ('enter',), ('note',)),
        Rule('no-ethernet', r'No Ethernet card was found on the system|Ethernet card not found', ('enter',), ('note', 'select')),
        Rule('ethernet-driver', r'Driver needed by your Ethernet card', ('select', r'no ethernet card', ''), ('select',)),
        Rule('grub-device', r'Device for boot loader installation', ('select', r'/dev/vda', ''), ('select', 'string')),
        Rule('scan-media', r'Scan another CD or DVD\?', ('no',), ('boolean',)),
        Rule('installation-complete', r'Installation complete', ('enter',), ('note',)),
    ]


def decide(form, rule_list):
    """Return (rule, action) for a recognised form, ('progress', ('wait',)) for a progress
    screen, or None for anything unknown."""
    if form.kind in ('progress', 'blank'):
        return Rule('progress', '', ('wait',)), ('wait',)
    for rule in rule_list:
        if rule.matches(form):
            return rule, rule.action
    return None
