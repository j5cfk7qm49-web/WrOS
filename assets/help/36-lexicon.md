# Dictionary and thesaurus

Open **Dictionary and thesaurus** from Writing tools or the command palette. It reads local **Princeton WordNet 3.0** index/data files, not a generated definition, internet service or model. Native spelling stays independent. Opening never changes the original editor or its selection.

Type a word or short phrase and press Enter. Selection/current word may prefill the field, but opening does not run a query. Backspace edits; Delete clears. Pasted text is query input only: it cannot run commands. Missing, corrupt or unsupported data is visibly unavailable, not an invented or silently downloaded result. No query/result is saved as project prose or history.

Up/Down chooses a specific part-of-speech/sense; Enter opens its actual gloss and completes genuine read-only lesson 25. Merely opening, typing or waiting cannot complete the lesson. Index-verified morphology is labeled with the resolved lemma: it does not correct the manuscript. At most 64 senses are displayed, with an explicit truncation notice.

Inside a sense, Tab or Left/Right switches **synset words/synonyms** and **explicit antonym pointers**. Up/Down chooses a word; PgUp/PgDn scrolls long content. Antonyms belong to the particular queried word and sense, not every synonym. Missing opposites are never inferred. A synonym is not necessarily interchangeable in this sentence or register. Historical dictionary senses are not endorsements or universal rules for fiction.

**c** explicitly copies a selected word to the existing clipboard. It does not insert, replace, clear a selection or own another editor. Escape returns through senses and query to the original editor. Use ordinary native selection/paste/undo if you choose to edit there; this is explicit Paste origin, not inferred typing or an unchecked one-click suggestion. Pasted commands, cut/undo or dragging in the reader cannot mutate a hidden source.

There is one active dictionary lookup and one coalesced newest request. Stale responses are discarded. File I/O/parsing is off the input/render path. Closing cancels publication without blocking on a thread join; a read-only lookup may finish asynchronously. No background startup query or runtime download is required.

The packaged data path is `/usr/share/wordnet` (`wordnet-base`). An explicit `WRITINGDECK_WORDNET_DIR` can point to another local WordNet 3.0-format data folder; custom data is format-checked, not authenticated as the reviewed package. CLI: `grimoire writingdeck lexicon "hot" --data /usr/share/wordnet` is read-only JSON. The upstream WordNet notice remains with all database copies and in the installed notices. No overall literary-quality score is produced.
