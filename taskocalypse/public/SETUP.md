# Taskocalypse setup

A React + Three.js town with a Rust/Axum server and SQLite database.

The hosted Sites version is a static demo. Sites hosting does not run this native Rust server. To use accounts and real Gmail, run the complete app below or deploy its Docker image to a container host with persistent storage. The Rust server serves the built frontend and API on the same origin.

## Start locally

Requirements: Node.js 22+, Rust stable, and a C/C++ toolchain for bundled SQLite (on Windows use Rust GNU with MinGW, or Rust MSVC with Visual Studio C++ Build Tools). Docker is an alternative.

From the taskocalypse directory:

```sh
npm ci
npm run build
```

Copy `.env.example` to `.env`. Generate a random encryption key:

```sh
node -e "console.log(require('crypto').randomBytes(32).toString('base64'))"
```

Paste that value into `TOKEN_ENCRYPTION_KEY` in your local `.env`. Keep this key private and stable; replacing it invalidates stored Google tokens. Do not commit `.env` or your database.

```sh
cargo run --manifest-path server/Cargo.toml
```

Open **http://localhost:8080**. Real sign-in requires the OTP sender configuration below. Email/password accounts do not need Google credentials until connecting Gmail. Passwords need at least 12 characters; use a unique app password, not your Google password. Password recovery is not implemented.

## Email verification and three-hour activations

Configure these deployment secrets once, for your application:

1. Create a Resend API key and verify a sender domain using [Resend's setup](https://resend.com/docs/send-with-nodejs).
2. Set `OTP_API_KEY` to that key and `OTP_FROM` to an address on that verified domain, for example `Taskocalypse <verify@yourdomain.com>`.
3. Restart the Rust server. If the key or sender is missing, sign-in fails closed; there is no production OTP bypass or console code.

The server uses Resend's [send-email API](https://resend.com/docs/api-reference/emails/send-email). The email goes to the user's registered email for password sign-in, or the Google-verified mailbox for Google sign-in. The user enters a code sent by Taskocalypse; the app never requests a Google recovery code, Google two-factor code, or Google password. Users do not provide OAuth client secrets or mail-delivery keys.

The flow is:

1. Accept the terms and sign in with Google or your app password.
2. Enter the emailed six-digit OTP and accept automatic inbox syncing.
3. The backend activates access for **10,800 seconds (three hours)** from successful OTP verification.
4. If Gmail is connected, a server worker starts syncing the first inbox page, normally within five seconds, and refreshes it about once per minute. It also runs while the browser is closed. Load more letters retrieves additional pages on demand. The worker never sends replies, archives, or marks messages read.
5. Every authenticated request, token refresh, and synchronization checks the stored deadline. Page reloads, token refreshes, and another verified login during an active window do **not** extend it.
6. At the deadline, access is rejected immediately. A separate worker removes local Google tokens, encrypted inbox cache, and sessions within its five-second cleanup interval. If the server was stopped, startup cleanup runs before it begins serving requests. Browser countdowns are only a convenience; enforcement is on the server.
7. Start a new activation by signing in and verifying a new code. Reconnect Google when needed because the expired activation's tokens have been deleted.

OTPs expire after ten minutes, are bound to an HttpOnly browser cookie, and can be used only once. Only a keyed HMAC of a code is stored; plaintext codes are sent to the email provider, never returned by an API or logged. Five wrong guesses invalidate a challenge. Resending requires 60 seconds and does not reset its ten-minute deadline or guess budget. Each account is limited to five sends and fifteen failed guesses per hour, in addition to IP-based authentication throttling.

Google tokens received during OAuth are encrypted in a pending record, never used to read mail before OTP verification, and discarded when the pending verification expires. Signing out ends the activation across all sessions, stops sync, and deletes the local tokens/cache immediately. Account identity, password hashes, and earned XP remain for future logins. Account deletion removes those records too.

Expiry removes the app's usable local credentials; it does not guarantee remote revocation of Google's consent grant. Users can revoke that grant in [Google account connections](https://myaccount.google.com/connections). An email action already accepted by Gmail just before expiry cannot be recalled by the timer; uncertain submissions remain protected against duplicate retries.

This release uses one Rust process with a persistent SQLite database. Run one server replica; horizontal scaling requires a shared job/locking design. Keep the host clock synchronized and the server running for background work. Sync scheduling can take longer under load or Google outages, but the access deadline still applies. Email delivery and Gmail must be exercised with your configured providers before a public launch; automated tests use local mock providers.

For hot reload: set `APP_URL=http://localhost:5173`, run the Rust server, then run `npm run dev`. Open localhost:5173 (Vite proxies `/api` to port 8080). Update the Google callback URL to match this origin.

## Connect Google and Gmail

1. Create or select a project at https://console.cloud.google.com/.
2. Enable **Gmail API** in APIs & Services → Library.
3. Configure Google Auth Platform branding, audience, and data access. For testing, keep the app in Testing and add your Google email as a test user.
4. Create an OAuth client of type **Web application**.
5. Add this authorized redirect URI exactly: `http://localhost:8080/api/auth/google/callback`.
6. Set `GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET` in `.env`, using the client credentials from Google. Never put the client secret in frontend code or chat.
7. Restart the Rust server. Accept the terms and click **Continue with Google**. If you already made an email/password account with the same address, sign in first and connect Google in Settings.
8. Grant Gmail permission, then enter the Taskocalypse verification code emailed to that Gmail address. Inbox syncing starts automatically after verification; mailing-list, forum, and social-category mail appears in the festival square.

Requested scopes: `openid email profile https://www.googleapis.com/auth/gmail.modify`. This supports reading, replying, archiving, and marking messages read. Google may require verification for a public app, and testing-mode authorizations may expire. Follow Google's requirements before opening access to other users.

Official references:
- https://developers.google.com/identity/protocols/oauth2/web-server
- https://developers.google.com/workspace/gmail/api/guides/sending
- https://developers.google.com/workspace/gmail/api/guides/threads

## What each resident does

- **Milo:** reads your inbox in pages of 40 messages, sends real threaded replies through Gmail, archives, and marks read. Mail is rendered as text. Attachments are not downloaded. Use Load more letters to reach older messages, or Refresh to reload your inbox.
- **Sunny:** shows announcements detected by List-Id headers or Gmail's forums/social labels. Click the sky banner to open the noticeboard. Classification follows these rules; it is not an AI classifier.
- **Poppy:** uses the browser File System Access API in desktop Chrome/Edge to move loose files from a selected folder into category subfolders. Review the plan first. Copy completes and size is checked before the original is removed. Collisions stop the process without overwriting. Operations are not atomic; if interrupted, review the source and destination before retrying.
- **Jasper:** imports a CSV with `description`, `amount`, and optional `date`, records your categories locally, and exports the reviewed ledger. No bank connection. Amounts retain the input currency.
- **Ada:** finds trailing whitespace and downloads a cleaned code file. This is a narrowly scoped cleanup tool, not a full Rust/JavaScript linter. Original files are not overwritten.

WASD/arrows walk on the paths. Explore Serenity Valley to find six keepsakes; each discovery earns XP and stays found on this browser. If WebGL is unavailable, the town provides an accessible keepsake list instead. E talks to the nearest resident. Click name labels or use the sidebar for direct access. Drag to orbit; scroll or use +/- to zoom. Day/night mode follows local time by default, with manual day and night options in Settings. Sound and reduced motion are also available.

## Docker deployment

Configure `.env` first, then:

```sh
docker compose up --build -d
```

The database is stored in the `town-data` named volume. Back up that volume and your encryption key separately; choose a backup retention policy appropriate for account records. For an Internet deployment, set `APP_URL` to the exact HTTPS origin, configure the matching Google redirect URI, put the container behind an HTTPS reverse proxy, and keep the frontend/API on one origin. Non-loopback HTTP origins are rejected. Secure cookies are enabled for HTTPS. The auth limiter uses the direct peer IP and does not trust client-supplied forwarding headers; behind a proxy, users share the peer-IP rate bucket. Do not use the static Sites URL as the Rust callback unless you have independently configured an appropriate same-origin reverse proxy.

## Checks

```sh
npm run build
npm test
cargo test --manifest-path server/Cargo.toml
npx playwright install chromium
npm run test:e2e
```

## Operational notes

- Argon2 password hashing; hashed opaque session IDs; HttpOnly, SameSite cookies; exact-origin checks and required custom headers for mutations; per-IP authentication throttling.
- PKCE and one-time, browser-bound OAuth state; encrypted Google access/refresh tokens with account-bound authenticated encryption; token refresh only within the verified activation.
- Real mail actions require explicit buttons. A durable action reservation blocks duplicate sends, including uncertain network outcomes. If delivery is uncertain, inspect Gmail; the app deliberately will not retry that message's reply. A later new incoming message can receive its own reply.
- XP is persisted for Gmail actions and awarded only after Gmail reports success. Demo progress persists in this browser. Local chore XP lasts for the visit on signed-in accounts. Quest history for signed-in accounts is limited to the current visit.
- Adventure keepsake discoveries and the selected day/night mode are saved in the current browser. They do not sync between devices.
- A single SQLite-backed server instance serializes mail operations and runs background inbox sync and expiry cleanup. Multi-instance deployment, account recovery, and production observability need additional work before broad release. This implementation is tested, not a guarantee of perfect security.
- No live Gmail connection can be tested until you configure your own Google project and authorize a test account.

### Upgrade notes and integration tests

On the first run of this upgrade, the schema migration invalidates all legacy twelve-hour sessions and removes their Gmail tokens. Accounts and progress are retained. Users must sign in, complete OTP verification, and reconnect Google. Later restarts preserve each unexpired activation's original deadline.

`node tests/backend-live.mjs` runs a browser-to-Rust check using a temporary database and a local HTTP email sink; it sends no real mail. Build the Rust debug binary first with `cargo build --manifest-path server/Cargo.toml`. `OTP_API_URL` is reserved for that local integration test and may point only to loopback HTTP when `APP_URL` is also loopback. Production uses the fixed HTTPS Resend endpoint. There is no configurable OTP lifetime or activation duration that can accidentally disable the three-hour policy.
