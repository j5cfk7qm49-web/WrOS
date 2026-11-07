# Setting up WrOS's Google sign-in (one time, for the project owner)

WrOS links Google Drive with the writer's phone: the laptop shows a QR code,
the phone signs in to Google, and the approval comes back to the laptop. For
that, the WrOS project needs its own Google sign-in. This is done once, by the
project owner, and every WrOS install then uses it. It works from a phone
browser; allow about 15 minutes.

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

(A second client, "TVs and Limited Input", was made on 2026-11-04 for a
fallback that Google turned out not to allow: its device flow refuses the
hidden app folder. It is not needed and can be deleted.)

## 5. Hand it to WrOS

Hand over the downloaded JSON. Google does not treat this client's secret as
secret, because every installed copy carries it. It is still kept out of the
public repository: it lives in a local file that the release build puts into
the package. The client ID is public anyway, since it appears in every
sign-in address.

## Afterwards

- **Budget alert:** Billing › Budgets & alerts › a small budget (for example
  $1) with email alerts, so unusual use is noticed. Drive use itself is free.
- **If the client is ever misused:** create a new one, ship it in a signed
  WrOS update, and each writer links again once with the QR code.
