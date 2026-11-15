# VM test signing key (not a release key)

A throwaway OpenPGP key that exists so the update VM test can sign bundles and have a test
machine accept them. Its secret half is committed on purpose. No real WrOS system trusts it:
release trust lives only in `os/config/update-trust/`, which this key is not part of.

| File | What it is |
|---|---|
| `wros-archive-keyring.gpg` | The public key, in the same format and under the same name as the release keyring |
| `update-signers` | The pinned fingerprint, `4FA22B69E08645899F5ECE30CC402B6965BE4400` |
| `test-signer.sec.asc` | The secret key, armoured, without a passphrase |

A test machine is given this directory as its update trust directory instead of
`os/config/update-trust/`. The installed system and the live medium never are. A test signs with
`gpg --import test-signer.sec.asc` in a temporary GnuPG home inside the VM or the test builder.

Do not edit or replace these files.
