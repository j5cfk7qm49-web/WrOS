# Setting up WrOS's Google sign-in (one time, for the project owner)

WrOS links Google Drive with the writer's phone: the laptop shows a QR code,
the phone signs in to Google, and the approval comes back to the laptop.
The project owner supplies WrOS's Google clients to release builds. Builds
without those clients, including ordinary CI/test builds, omit phone linking
and retain manual rclone authorization.

Why it is needed at all: rclone's shared Google sign-in is being retired and
stops working during 2026.

## 1. Create the project

1. Open <https://console.cloud.google.com> and sign in.
2. Project menu (top left) › **New project**. Name: `WrOS`. Create, then select it.

## 2. Turn on the Drive API

**APIs & Services › Library** › search **Google Drive API** › **Enable**.

## 3. Set up the sign-in screen

Open **Google Auth Platform** (search for it if it is not in the menu) › **Get started**.

1. **App name:** `WrOS`. **User support email:** yours.
2. **Audience:** **External**.
3. **Contact information:** your email. Agree, **Create**.
4. **Branding:**
   - Application home page: `https://j5cfk7qm49-web.github.io/WrOS/`
   - Privacy policy: `https://j5cfk7qm49-web.github.io/WrOS/privacy/`
   - Authorized domains: `j5cfk7qm49-web.github.io`
   - Leave the logo empty, because a logo sends the app to Google's review.
   - Save. Google needs both pages to load, so they must already be on the
     site (they live in `pages/`).
5. **Data access › Add or remove scopes:** add
   `https://www.googleapis.com/auth/drive`. Save.
6. **Audience › Publish app** › confirm, so it says **In production**.
   (Left in Testing, every Drive link would expire after 7 days.)

WrOS stays under Google's personal-use exemption (fewer than 100 users), so no
verification is needed; sign-in shows an "unverified app" notice to click
through ("Advanced" › "Go to WrOS").

## 4. Create the client

**Google Auth Platform › Clients › Create client:**
- **Application type:** Web application.
- **Name:** `WrOS linking`.
- **Authorized redirect URIs:** `https://j5cfk7qm49-web.github.io/WrOS/link/`.

Create it, then download its JSON file.

Keep a second client of type TVs and Limited Input in the same Google project.
WrOS uses it as the mailbox client with the drive.file scope to retrieve the
phone's approval from an ordinary Drive file. It does not use the hidden
app-data folder. Phone linking requires both clients.

## 5. Hand it to WrOS

Combine both clients into
`{"linking":{"client_id":"…","client_secret":"…"},"mailbox":{"client_id":"…","client_secret":"…"}}`.
Set `WROS_GOOGLE_CLIENTS` to that file and build the ISO or update bundle with
`--impure`. The NixOS build installs it as `/etc/wros/google-client.json`.
A raw Google client download is not this format. Keep the combined file out of
the public repository.

## Afterwards

- **Budget alert:** Billing › Budgets & alerts › a small budget (for example
  $1) with email alerts, so unusual use is noticed. Drive use itself is free.
- **If the client is ever misused:** create a new one, ship it in a signed
  WrOS update, and each writer links again once with the QR code.
