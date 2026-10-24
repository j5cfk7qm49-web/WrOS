#!/usr/bin/env python3
"""Regenerate self-contained printable HTML from the authoritative offline help."""
from pathlib import Path
import hashlib
import html
import re

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / 'assets/help/24-every-key.md'

def inline(text):
    text = html.escape(text)
    return re.sub(r'`([^`]+)`', r'<kbd>\1</kbd>', text)

def main():
    blocks = []
    table = False
    for line in SOURCE.read_text().splitlines()[1:]:
        if line.startswith('|'):
            cells = [c.strip() for c in line.strip('|').split('|')]
            if all(re.fullmatch(r'[-: ]+', c) for c in cells):
                continue
            if not table:
                blocks.append('<table>')
                table = True
            blocks.append('<tr>' + ''.join('<td>'+inline(c)+'</td>' for c in cells) + '</tr>')
        else:
            if table:
                blocks.append('</table>')
                table = False
            if line.startswith('## '):
                blocks.append('<h2>'+html.escape(line[3:])+'</h2>')
            elif line:
                blocks.append('<p>'+inline(line)+'</p>')
    if table:
        blocks.append('</table>')
    digest = hashlib.sha256(SOURCE.read_bytes()).hexdigest()
    result = '''<!doctype html><html lang="en"><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<meta name="source-sha256" content="HASH">
<title>WritingDeck — printable keyboard reference</title>
<style>
*{box-sizing:border-box}body{margin:0;background:#edece7;color:#242923;font:14px/1.4 system-ui,sans-serif}main{max-width:1000px;margin:28px auto;padding:32px 40px;background:#fff;border-top:8px solid #365b47}h1{font-size:32px;margin:0}header p{color:#586258;margin:6px 0 18px}.badge{font:700 11px system-ui;letter-spacing:.12em;color:#365b47}aside{padding:12px 15px;background:#edf3ee;border:1px solid #cfdbd1;margin:20px 0}.keys{columns:2;column-gap:32px}h2{font-size:16px;margin:18px 0 5px;break-after:avoid}p{margin:7px 0}table{border-collapse:collapse;width:100%;font-size:12px}tr{break-inside:avoid}td{border-bottom:1px solid #ddd;padding:4px 3px;vertical-align:top}td:first-child{width:34%}tr:first-child{font-weight:bold}kbd{font:inherit;font-weight:600}footer{border-top:1px solid #ddd;padding-top:12px;margin-top:20px;font-size:11px;color:#555}@media(max-width:680px){main{margin:0;padding:20px}.keys{columns:1}}@media print{@page{margin:12mm}body{background:white;font-size:10pt}main{margin:0;padding:0;max-width:none;border:0}h1{font-size:24pt}.keys{columns:2;column-gap:7mm}aside{background:white;margin:10px 0}table{font-size:8pt}h2{font-size:11pt}td{padding:2px}footer{font-size:8pt}header p{margin-bottom:8px}}
</style><main><header><span class="badge">WRITINGDECK · OFFLINE REFERENCE · 0.1.0-DEV</span>
<h1>Your keys, your writing desk.</h1><p>QWERTY · Colemak · Colemak-DH — use the logical key label after Linux layout translation. Print from your browser.</p></header>
<aside><strong>F12</strong> home &nbsp; <strong>F10</strong> outline &nbsp; <strong>Ctrl-D</strong> strict focus.<br>
<strong>F11 is not yet an editable second pane.</strong> System → Keyboard layout has a 15-second preview: Enter keeps it, Escape/timeout reverts it. System → Keyboard shortcuts adds safe Ctrl+Alt+A–Z or F10–F12 aliases; legacy keys stay available. Avoid AltGr conflicts. No double remapping.</aside>
<div class="keys">BODY</div><footer>Generated from assets/help/24-every-key.md. Plain-text searchable help is built in (F1). Some terminals cannot distinguish Ctrl-I/Tab or Ctrl-M/Enter; use menu/palette alternatives. Optional music needs its own setup; writing never needs a network. This foundation is not a production installation.</footer></main></html>
'''.replace('HASH',digest).replace('BODY','\n'.join(blocks))
    (ROOT/'docs/keyboard-reference.html').write_text(result)
    print('wrote docs/keyboard-reference.html')

if __name__ == '__main__':
    main()
