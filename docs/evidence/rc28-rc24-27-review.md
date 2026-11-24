# RC28 review of RC24–27

Reviewed the changes from `fc27bb5^` through RC27 `821e8da`. The review
covered project opening and local saves, MLA export, Brain journal and cache
recovery, Review identities and due-card queries, and Library editions and
attachments.

| Finding | RC28 change |
| --- | --- |
| Shelf navigation called the quit routine, which explicitly waits for a Drive sync. | Project switching waits for the local save. The departing project's worker and its result stay with that project. Quit keeps its final-sync gate. |
| Desk export ignored the save result, including the case where local changes were still waiting for a file to come back. | A failed or pending save pauses export and keeps the current export choices. |
| The Library picker committed a Brain note and each typed link in separate journal writes. | Text, new links and any Review card IDs now share one operation. Older entries still replay. A cache from an older version is rebuilt so it includes the new links. |
| Removing a cloze could give the remaining cloze the removed card's history when their answers were identical. | Match an unchanged prompt and answer before pairing clozes by answer. |
| The due-card query carried full definitions through its window and sort. Text filters repeatedly queried the full-text index. | Count and select IDs first, then fetch one definition. Resolve text matches once per query. |
| Importing a duplicate attachment wrote a temporary copy before discovering the existing file. | Hash first and reuse the verified blob. A new blob is checked again as it is written and is published without overwriting a concurrent import. |

On the WSL build host, an in-memory SQLite 3.53.3 comparison used 8,000 card
definitions of 12,000 bytes, varied due times, and nine runs per query. Both
queries returned the same first card and 4,476 due cards. Median query time
fell from 150.9 ms to 5.7 ms. These are query timings on that fixture.

The regression tests cover failed Drive sync during a project switch,
export after a pending save clears, an unchanged cloze among identical
answers, attachment reuse on a read-only directory, and a cache rebuilt
from a note saved with a link and Review card. See the Brain, Core and
desk commands in [Testing](../TESTING.md).

## Desk and menu follow-up

Six added cases failed against the earlier RC28 build. They cover hidden
project actions, a search result left behind another view, inconsistent View
destinations, Find anything clipping its selection at 80×16, a lost Library
tag filter, and a catalog preview appearing during an unfinished edit.
View navigation and manuscript isolation are checked in the desk tests.
Catalog replies use local fixtures. The HP Stream was offline for this pass.
