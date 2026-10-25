#!/usr/bin/env python3
"""Capture real non-root demo-only workflow and journal terminal frames.

Build cargo --no-default-features first. Uses the shared compiled-app PTY
fixtures; no host devices/services or existing project directories.
"""
import json, os, sys, tempfile
from pathlib import Path
from datetime import datetime, timezone
ROOT=Path(__file__).resolve().parents[1]
os.chdir(ROOT)
sys.path.insert(0,str(ROOT/'os/tests'))
from test_journal_pty import fixture
from test_secondary_pty import TerminalProcess


def main():
    if os.geteuid()==0:
        raise SystemExit("Capture must run as a normal user, not root.")
    stamp=datetime.now(timezone.utc).isoformat()
    intro=f'Actual non-root compiled-app PTY, 80x24, isolated newly created demo/HOME.\nCaptured {stamp}.\nANSI cursor/erase/UTF-8 cell reconstruction; not a full terminal emulator.\nNo physical keyboard or VM workflow claim. Original-source fixtures only.\n'
    frames=[]
    def frame(term,label,*needles):
     def complete():
      rows=term.screen.text.splitlines()
      return len(rows)==24 and all(n in term.screen.text for n in needles) and rows[0].startswith('┌') and rows[-1].startswith('└') and all(row.startswith('│') and row.endswith('│') for row in rows[1:-1])
     term.until(complete,'complete cleared bordered '+label+' frame')
     text=term.screen.text
     assert all(len(row)==80 for row in text.splitlines())
     return label+'\n'+text+'\n'
    with tempfile.TemporaryDirectory(prefix='wd-workflow-evidence-') as tmp:
     project,home,primary=fixture(Path(tmp));original=primary.read_bytes()
     source=project/'notes/scratchpad/000001.md';target=project/'manuscript/90-Scratchpad-000001.md';handoff=project/'notes/handoff.md';meta=project/'.grimoire/writingdeck/workflow.json'
     term=TerminalProcess(project,home,80,24)
     try:
      term.shows('The Brass Key');term.send(b'\x0bThe Brass Key\r\x0bScratchpad\rn');term.paste('Original river idea, kept as ordinary Markdown. ');term.send(b'\x13')
      term.until(lambda: source.is_file() and b'Original river idea' in source.read_bytes(),'original scratch save');saved=source.read_bytes()
      term.send(b'\x0bScratchpad\rlThe Brass Key\rp')
      term.until(lambda: target.is_file() and json.loads(meta.read_text())['notes'][0]['promotion']['phase']=='ready','Ready snapshot receipt')
      frames.append(frame(term,'ORIGINAL SCRATCHPAD AND INDEPENDENT READY SCENE','Scratchpad · original Notes','Promotion: Ready','Original river idea','Tab archive'))
      assert source.read_bytes()==saved and primary.read_bytes()==original
      scene=target.read_bytes();term.send(b'\x0bSession handoff\r\r');term.paste('Next time, follow the river beyond the brass door. ');term.send(b'\x13')
      term.until(lambda: handoff.is_file() and b'Next time, follow the river' in handoff.read_bytes(),'original intention save');saved_handoff=handoff.read_bytes()
      term.send(b'\x0bSession handoff\rs')
      term.until(lambda: json.loads(meta.read_text())['handoff']['show_on_open'] is True,'explicit saved intention arm')
      term.send(b'\x11');term.until(lambda: term.process.poll() is not None,'normal workflow exit');assert term.process.returncode==0
     finally:term.close()
     term=TerminalProcess(project,home,80,24)
     try:
      frames.append(frame(term,'SEPARATE-PROCESS NEXT-OPEN ORIGINAL HANDOFF','Session handoff · original intention','Next-open display: true','Context: manuscript','Next time, follow the river','Esc'))
      assert handoff.read_bytes()==saved_handoff and source.read_bytes()==saved and target.read_bytes()==scene
      term.send(b'\x0bWorkspaces\rbApply Research starter\r\t');term.paste('Independent research original, not a copied card body. ');term.send(b'\x13')
      research=project/'research/workspace.md';term.until(lambda: research.is_file() and b'Independent research original' in research.read_bytes(),'Research original save')
      term.send(b'\x0bWorkspaces\rpz')
      # Pin-set editor has the dashboard's original border and read-only source input guard.
      term.screen_shows('Pin set · Drafting');term.screen_shows('No cards; create references')
      term.until(lambda: 'Space toggle' in term.screen.text,'pin-set footer rendered')
      frames.append('EXPLICIT WORKSPACE PIN-SET EDITOR (DRAFT NONE; ENTER SAVES)\n'+term.screen.text+'\n')
      term.send(b'\r\x11');term.until(lambda: term.process.poll() is not None,'normal starter exit');assert term.process.returncode==0
      assert primary.read_bytes()==original and source.read_bytes()==saved and target.read_bytes()==scene and handoff.read_bytes()==saved_handoff
     finally:term.close()
    Path('docs/evidence/workflow-pty-screen.txt').write_text(intro+'\n'+'\n'.join(frames))

    # Recapture retained M4b readers using the current binary, not an old screenshot.
    frames=[]
    with tempfile.TemporaryDirectory(prefix='wd-journal-evidence-') as tmp:
     project,home,primary=fixture(Path(tmp));term=TerminalProcess(project,home,80,24)
     try:
      term.shows('The Brass Key');term.send(b'\x0bThe Brass Key\r');term.paste('Saved manuscript checkpoint for journal evidence. ');term.send(b'\x13')
      logdir=project/'.grimoire/writingdeck/journal'
      term.until(lambda: len(list(logdir.glob('*.json')))==1,'actual measured saved observation')
      log=next(logdir.glob('*.json'));fact=json.loads(log.read_text())['facts'][0];assert fact['kind']=='saved-checkpoint'
      term.send(b'\x0bWriting journal\r')
      frames.append(frame(term,'RECORDED FACTS','Writing journal','Recorded facts','Saved checkpoint','initial 25 words','Esc'))
      term.send(b'\t\r');term.paste('Original reflection: keep the next sentence concrete. ');term.send(b'\x13')
      reflection=project/'notes/journal'/f'{log.stem}.md';term.until(lambda: reflection.is_file() and b'Original reflection:' in reflection.read_bytes(),'actual original reflection')
      term.send(b'\x0bWriting journal\r\t')
      frames.append(frame(term,'ORIGINAL REFLECTION (SEPARATE MARKDOWN SOURCE)','Writing journal','Original reflection','keep the next sentence concrete','Esc'))
      term.send(b'\x11');term.until(lambda: term.process.poll() is not None,'normal journal exit');assert term.process.returncode==0
     finally:term.close()
    Path('docs/evidence/journal-pty-screen.txt').write_text(intro+'\n'+'\n'.join(frames))
    print('Captured current-binary workflow and retained journal terminal frames.')

if __name__=="__main__":
    main()
