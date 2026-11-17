# Revision

Tools for going back over a draft.

## Next scene still in draft

*Next scene still in draft*, in *Tools › Revision* or `Ctrl-K`, opens the next scene in book order whose status isn't *revised* or *done* — empty scenes are skipped, and it goes round to the start when it reaches the end. Work through a revision pass by setting each scene's status as you finish it (`s` on the corkboard steps it, or edit `status:` in the frontmatter).

## Echo words

*Echo words* lights up a word you've used again within about forty words of the last time — the "the lamp… the lamp" that's easy to miss. Common little words and your characters' names are left out. Turn it on and off from *Tools › Revision* or `Ctrl-K`. It's off to begin with, and it only marks the page: nothing in the scene changes until you do.

## Read aloud

Hearing your prose catches what the eye skips: the missing word, the sentence that runs out of breath, the rhythm that's off. *Tools › Read aloud* (or `Ctrl-K` › *Read aloud from here*) reads the scene out loud from the sentence you're on, and highlights each sentence as it goes.

- `Space` pauses, and `Space` again goes on.
- `←` and `→` go back or forward a sentence; `↑` and `↓` do too.
- `Esc` stops. The cursor stays where it stopped, so you can fix what you heard.
- `m` marks the sentence in Revision notes.
- With Read chapter aloud or Read manuscript aloud, `n` and `p` move between queued scenes.

Notes (`%% … %%`) are never read out, and neither are the `*` and `_` around italics.

### The voice

WrOS reads with whatever voice your computer has, and nothing leaves your computer:

- **On a Mac** and **on Windows**, the computer's own voice, already there.
- **On WrOS**, eSpeak NG, which WrOS includes.
- **On other Linux desktops**, Speech Dispatcher or eSpeak if either is installed (`sudo apt install espeak-ng`, `sudo dnf install espeak-ng`).
- **Piper** is optional and is not included in the WrOS image. A custom build needs a `piper` executable and a voice model with its matching `.onnx.json` file. WrOS searches `~/.local/share/piper-voices` for models. The supplied WrOS voice is eSpeak NG.

When there's more than one, WrOS picks the most natural. *Settings › Writing › Reading voice…* chooses a different one.

## Other passes

- *Check names for near-miss spellings* finds names that have drifted (see *The notebook and names*).
- `Ctrl-T` lists every note and TK you left for yourself.
- `H` shows how a scene has changed.
- *Writing sessions* show what changed in each sitting.
