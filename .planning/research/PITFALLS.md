# Pitfalls Research

**Domain:** RLCraft (Forge 1.12.2) server on Raspberry Pi 5 (ARM64) + offline-mode custom auth + Tauri launcher (Windows/macOS)
**Researched:** 2026-08-27
**Confidence:** MEDIUM-HIGH (server/Forge/Java findings well-corroborated across multiple independent sources; Tauri/macOS findings HIGH confidence from official docs and GitHub issues; some ARM64-specific RLCraft performance numbers are LOW confidence — sparse direct data, extrapolated from general modded-server and Pi benchmarks)

## Critical Pitfalls

### Pitfall 1: Underestimating RLCraft's CPU cost on ARM — treating it like a RAM problem

**What goes wrong:**
The Pi 5 has 15 GB RAM, which looks generous next to RLCraft's usual 6-8 GB heap recommendation, so it's tempting to assume the server will "just work." In practice RLCraft is CPU-bound, not RAM-bound: dense mob AI (Lycanites Mobs, dragons), chunk generation cost (biome/dimension mods), and Forge's own per-tick overhead saturate 1-2 cores long before RAM runs out. Reports of Pi-class ARM boards running RLCraft show "Can't keep up! Running Nms behind, skipping ticks" within minutes of a few players exploring simultaneously — a single-thread tick-rate problem RAM cannot fix.

**Why it happens:**
Minecraft's server tick loop is fundamentally single-threaded for world/entity logic; RLCraft's mods multiply per-tick entity and AI work far beyond vanilla. The Pi 5's Cortex-A76 cores are respectable per-core but nowhere near a modern x86 desktop's single-thread throughput, and Forge 1.12.2 (2017-era mod loader) has none of the tick-parallelization work that Paper/Fabric-adjacent ecosystems added later.

**How to avoid:**
- Budget for real TPS testing with 2-3 simultaneous players (not a solo smoke test) before calling the server phase "done."
- Plan the server config from day one for reduced view-distance (6-8 instead of default 10), capped mob caps, and simulation-distance tuning appropriate to 1.12.2 (`spawn-monsters`, mob cap tweaks via config, not server.properties `simulation-distance` which doesn't exist pre-1.18).
- Install a tick-lag mitigation mod compatible with Forge 1.12.2 (e.g., FoamFix for memory, and consider Cleanview/AI-limiting mods) — validate compatibility with RLCraft's mod list before committing.
- Set expectations with the friend group: 5-7 players on a Pi 5 running RLCraft is a real risk area, not a guaranteed win — plan a fallback (reduce difficulty features, disable the most AI-heavy dimensions/mobs) if TPS is unacceptable.

**Warning signs:**
- "Can't keep up!" console warnings with more than one player online.
- TPS below ~15 with 2+ players even in a plain overworld biome (before dragons/dungeons).
- High CPU on a single core (check `htop`/`top` per-core) while others sit idle — confirms single-thread tick bottleneck, not a RAM/GC issue.

**Phase to address:**
Server setup/performance-tuning phase — before onboarding the full friend group. Should include a dedicated load-test step.

---

### Pitfall 2: Wrong Java version breaks the server or client silently/loudly

**What goes wrong:**
Forge 1.12.2 (built against pre-modular Java) only runs on Java 8. Java 9+ changed internal classloader architecture (`AppClassLoader` no longer extends `URLClassLoader`), so Forge 1.12.2 throws `ClassCastException: class jdk.internal.loader.ClassLoaders$AppClassLoader cannot be cast to class java.net.URLClassLoader` or simply fails to launch ("unable to launch") on Java 9-25. The Pi 5 in this project already has Java 25 installed system-wide — running `java -jar forge-server.jar` without pinning to Java 8 will fail immediately.

**Why it happens:**
Forge 1.12.2's launch mechanism (LaunchWrapper) directly manipulates the JVM's URLClassLoader to inject mod classes onto the classpath at runtime — an implementation-detail hack that Java 9's module system and classloader redesign broke. Forge never backported a fix to 1.12.2; the real fix only arrived with Forge for MC 1.13+ (a different launch mechanism, ModLauncher).

**How to avoid:**
- Install a dedicated Java 8 runtime (Eclipse Temurin 8 or Azul Zulu 8 aarch64) alongside the system Java 25, and always invoke the server with an explicit path to the Java 8 binary (never rely on `PATH`/`JAVA_HOME` defaulting to system Java).
- Use the exact installer/version documented for the RLCraft server pack (commonly Forge 1.12.2-14.23.5.2860, matching the latest official RLCraft release) — do not substitute a different Forge build without checking the modpack's manifest.
- On the launcher side (client), auto-detect and download a Java 8 aarch64/x64 build per platform rather than trusting the user's system Java — this is already a stated requirement, but the pitfall is picking a JRE-only build when Forge's installer needs a full JDK, or picking a vendor without ARM64 macOS builds (see Pitfall 6).
- Wrap the server start script so a wrong-Java error fails fast and loud (exit code check, log line) rather than silently hanging.

**Warning signs:**
- `ClassCastException` mentioning `URLClassLoader` in server or client logs.
- Server process exits within seconds of `java -jar` with no visible mod-loading output.
- `java -version` on the box reports a version >8 when troubleshooting a "won't start" report.

**Phase to address:**
Server bootstrap phase (pin Java 8 path in start script) and Launcher Java-provisioning phase (bundle/download Java 8 explicitly, verify major version before launch).

---

### Pitfall 3: Offline-mode auth trusted client-side only (no server-side password enforcement)

**What goes wrong:**
Minecraft offline-mode servers generate UUIDs and accept usernames with zero verification against Mojang — anyone who can reach the server IP:port with a vanilla or modified client (not just the custom launcher) can log in as any username, including impersonating an existing registered player and hijacking their inventory/permissions/progress. If password checking lives only in the Tauri launcher (e.g., launcher won't "Play" without a correct password, but the Minecraft server itself accepts any offline-mode connection), the actual game server is wide open — the launcher's check is trivially bypassed by connecting with any other Minecraft client pointed at the server's IP.

**Why it happens:**
Vanilla/Forge's offline-mode flag only controls whether Mojang session-server verification happens; it does not add any authentication of its own. Developers new to offline-mode assume "my launcher gates access" without realizing the actual TCP/game port is unauthenticated to anyone who knows the IP — a very reasonable assumption to make incorrectly, since it's easy to conflate "controls how players start the game" with "controls who can join the server."

**How to avoid:**
- Enforce the password check at the **server** layer, not just the launcher: use a proven auth plugin/mod pattern (e.g., an AuthMe-style mod for Forge 1.12.2, or a lightweight custom mod that intercepts login and freezes/kicks unauthenticated players until they pass a password challenge) so that even a bare vanilla client connecting directly is rejected or sandboxed.
- If no clean 1.12.2 Forge auth mod exists, implement a minimal server-side check: on join, compare the connecting username+session token (or a shared secret embedded by the launcher, e.g., a signed short-lived token) against your own auth service before allowing movement/world access — never rely solely on "the launcher already checked."
- Treat the game server port as effectively public even if you don't advertise the IP — offline-mode + password-in-launcher-only is security theater against anyone scanning the IP or getting it from a friend-of-a-friend.
- Store passwords hashed (bcrypt cost ≥12, or Argon2id if the auth service is in a language with a good library) — never plaintext or reversible encryption, even for a "just friends" server.

**Warning signs:**
- Auth logic exists only in launcher Rust/JS code with no corresponding server-side gate.
- A vanilla Minecraft client (no launcher) can join and move around the world.
- Passwords stored in plaintext or a reversible format in the auth database.

**Phase to address:**
Auth/security phase — must be designed alongside (not after) the offline-mode server setup phase; verify with a manual "connect with vanilla client, no launcher" test before calling auth done.

---

### Pitfall 4: Redistributing Minecraft/mod files you don't have rights to serve

**What goes wrong:**
Two distinct legal traps: (1) serving the Minecraft client jar or Mojang-owned assets from your own file server instead of having the launcher fetch them from Mojang's official endpoints — a clear ToS violation; (2) bundling CurseForge-hosted mods (their jars) on your own file server without checking each mod author's distribution permissions — many mod authors explicitly disallow redistribution outside CurseForge/their own channels, and CurseForge's own modpack rules require third-party ("override") mods to be MIT/GPL-equivalent or pre-approved, precisely because this is a common violation.

**Why it happens:**
It's operationally easiest to just zip up the whole RLCraft client folder (Minecraft jar + assets + all mod jars) and serve it from your own box — one download, no external dependencies, but it silently violates both Mojang's EULA/ToS (client redistribution) and individual mod licenses.

**How to avoid:**
- Never host the Minecraft client jar or Mojang assets yourself — have the launcher perform the standard vanilla client bootstrap (download version manifest + jar + assets from Mojang's official CDN using the public version JSON, exactly like the official launcher does) before overlaying Forge and mods.
- For mods, prefer pulling from the CurseForge API (using the modpack's own manifest.json, which lists project/file IDs) at launcher-install time rather than mirroring jars on your own server — this respects each mod's distribution toggle and stays current automatically.
- If self-hosting mod jars for reliability/speed, audit each mod's license/CurseForge distribution flag first; only mirror ones explicitly marked redistributable, and keep the rest pulled live from CurseForge.
- Config files, your own server-side tweaks, and the modpack's own overrides folder are fine to host yourself — it's specifically Mojang assets and non-redistributable third-party mod jars that are the risk.

**Warning signs:**
- File server directory contains `minecraft.jar` or the full `assets/` tree.
- Mod jars checked into the file server's manifest without any record of checking each mod's CurseForge distribution permission.

**Phase to address:**
File-server/manifest design phase — decide the download strategy (Mojang-direct for vanilla, CurseForge-API-direct or license-audited mirror for mods) before building the manifest format.

---

### Pitfall 5: LWJGL 2 (Minecraft 1.12.2's renderer) is not Apple Silicon native — black screens, crashes

**What goes wrong:**
Minecraft 1.12.2 uses LWJGL 2.9.x, which predates ARM64/Apple Silicon support entirely. Running the client on an M1-M4 Mac either (a) requires forcing Rosetta 2 x86_64 emulation for the whole JVM, which is reported as "barely playable" (as low as 20 fps) because Rosetta cannot translate the AVX/AVX2 vector instructions some native LWJGL code paths use, or (b) requires manually swapping in community-recompiled ARM64-native LWJGL 2 + jinput binaries, which is a non-trivial patch that most players (and definitely a "minimalist launcher" target audience) cannot do themselves.

**Why it happens:**
LWJGL 2 shipped native `.dylib`/`.so`/`.dll` binaries per platform at a time (2010s) when ARM64 desktop/laptop chips didn't exist; the project has been effectively unmaintained for years, so no official arm64 macOS build exists — only community forks (e.g., work referenced from shadowfacts.net's "Run LWJGL 2 Natively on Apple Silicon") fill the gap.

**How to avoid:**
- Ship the launcher with the known-good ARM64-native LWJGL 2 + jinput replacement jars (sourced from a maintained community fork) bundled into the mod/library folder for Apple Silicon installs, rather than relying on Rosetta.
- Detect the Mac's architecture (`uname -m` → `arm64` vs `x86_64`) in the launcher and select the correct native-library set automatically — do not make the player choose.
- If native ARM64 LWJGL swap proves unreliable, fall back to explicitly launching the JVM under Rosetta (`arch -x86_64 java ...`) with a matching x86_64 Java 8 build, and set expectations that framerate will be materially worse on Apple Silicon than on Intel Macs or Windows.
- Test on real Apple Silicon hardware before shipping — this is one of the highest-risk, least-documented parts of the whole project and deserves explicit QA time, not just "should work."

**Warning signs:**
- Black screen after the Mojang splash on an M-series Mac.
- Crash logs mentioning `GLFW`, `jinput`, or native library load failures referencing `.dylib` architecture mismatch.
- Noticeably low FPS (sub-30) on Apple Silicon compared to the same settings on Windows/Intel Mac — sign Rosetta is active without acceleration.

**Phase to address:**
macOS launcher/client-bootstrap phase — needs its own explicit sub-task and hardware test pass (Apple Silicon Mac required), separate from the general "launcher launches the game" phase.

---

### Pitfall 6: Assuming any Java 8 download works for Apple Silicon macOS

**What goes wrong:**
Eclipse Temurin (Adoptium) — the most commonly recommended "just use Temurin" default — does not publish Java 8 builds for macOS arm64. A launcher that hardcodes Temurin as its Java-8-fetch source will either fail outright on Apple Silicon Macs or silently fall back to an x86_64 build (forcing Rosetta, compounding Pitfall 5).

**Why it happens:**
Adoptium's Java 8 support window predates their Apple Silicon build infrastructure investment; they only ship arm64 for newer LTS versions (11+), leaving Java 8 arm64 to alternate vendors.

**How to avoid:**
- Use Azul Zulu (most reliable, pioneered Apple Silicon Java 8 support) or BellSoft Liberica for the macOS arm64 Java 8 download in the launcher's Java-provisioning logic; use Temurin only for Windows x64 and (if needed) Intel macOS.
- Make the Java-vendor choice per-platform explicit and tested in code — a matrix of {Windows x64, macOS Intel, macOS Apple Silicon} → {vendor, download URL, checksum}, not a single hardcoded URL template assumed to work everywhere.
- Verify the downloaded JDK actually reports `arm64`/`aarch64` architecture after extraction (not just "Java 8" version) before considering provisioning successful.

**Warning signs:**
- Java provisioning "succeeds" (file downloads, unzips) but the game later runs under Rosetta anyway.
- `java -version` on the fetched runtime shows `x86_64` on an Apple Silicon machine.

**Phase to address:**
Launcher Java-provisioning phase — encode the per-platform vendor matrix as an explicit, tested config, not an assumption.

---

### Pitfall 7: Unsigned Tauri binaries get flagged as malware by Windows and macOS

**What goes wrong:**
An unsigned Tauri `.exe`/installer triggers a full-screen "Windows protected your PC" SmartScreen block by default — friends downloading the launcher will see a scary warning with the "Run anyway" option buried, and some unsigned Tauri builds get flagged as trojans by AV engines on first-seen-binary heuristics (reported in Tauri's own GitHub issues). On macOS, an unsigned/unnotarized `.app` triggers Gatekeeper's "cannot be opened because Apple cannot check it for malicious software" or, if quarantine+signature is inconsistent, the more confusing "app is damaged" error with no "Open Anyway" override — the app is simply unusable without a workaround.

**Why it happens:**
Both OS vendors gate first-run trust on code-signing reputation. Proper EV/OV code signing on Windows and Apple Developer ID + notarization on macOS cost money and setup (Apple Developer Program is $99/year minimum; Windows OV certs run somewhat more, EV significantly more) — easy to defer for a "just friends" project, but the resulting warnings look identical to real malware to a non-technical friend, killing the "press Play" UX goal.

**How to avoid:**
- Budget for at minimum an Apple Developer account ($99/yr) to sign + notarize the macOS build via Tauri's built-in signing/notarization support — this alone resolves the macOS "damaged"/Gatekeeper block cleanly.
- For Windows, either budget for a code-signing certificate (OV is enough to avoid the worst SmartScreen prompt over time as reputation builds; EV clears it immediately but costs much more) or explicitly document the one-time "More info → Run anyway" workaround for the closed friend group, since with only 5-7 users, cert cost may not be justified — write this into onboarding instructions.
- Submit built installers to VirusTotal before each release to catch AV false-positive flags early, and consider excluding common false-positive triggers (packing/compression settings) that the Tauri community has identified as noisy for AV heuristics.
- Because this is a closed friend-group tool, an acceptable lazy path is: sign macOS (cheap, fixes an otherwise-fatal error), and for Windows ship unsigned but document the SmartScreen click-through clearly in first-run instructions — revisit signing only if it becomes a recurring friction point.

**Warning signs:**
- A friend reports "Windows says this might be a virus" or "Mac says the app is damaged and won't open."
- VirusTotal scan on a fresh build shows new detections versus the previous build.

**Phase to address:**
Launcher packaging/distribution phase — decide signing strategy explicitly (don't default to "we'll deal with it later," since macOS Gatekeeper failure is a hard blocker, not just an annoying prompt).

---

### Pitfall 8: Modpack/Java download UX has no real progress feedback, looks frozen

**What goes wrong:**
RLCraft's client modpack (mods + configs + resources) plus a full Java 8 runtime is a large download (hundreds of MB to low GBs). If the launcher's download code just awaits a fetch/write-to-disk without streaming progress events to the UI, the "Play" button appears to hang for minutes with no feedback — a friend will assume it's broken and force-quit mid-download, corrupting the install.

**Why it happens:**
Tauri's plain HTTP client APIs and even its updater's default event system are not optimized for high-frequency large-file progress reporting out of the box — Tauri's own docs recommend using **channels** (not the general event bus) specifically for streaming download-progress data efficiently, because naive polling/eventing at high frequency can itself become a bottleneck or get dropped.

**How to avoid:**
- Implement download progress using Tauri's channel API (Rust side reads response body in chunks, emits progress via a channel to the frontend) rather than a single event fired only on completion.
- Show byte-level or percentage progress plus a cancel option in the launcher UI — silence during a multi-minute download is the single biggest driver of "it's broken" support requests from non-technical users.
- Write downloads to a temp file and atomically rename on completion (not directly to the final path) so an interrupted download never leaves a half-written file mistaken for a valid one.
- Verify each downloaded file against the manifest's hash after write, before marking it "installed" — catches partial/corrupted downloads from flaky home-network conditions.

**Warning signs:**
- UI shows a static "Downloading..." with no percentage/bytes for more than a few seconds.
- Support reports of "it just doesn't do anything when I press Play."

**Phase to address:**
Launcher download/update-manifest phase.

---

### Pitfall 9: Update manifest race conditions — player launches mid-deploy, gets a broken half-updated client

**What goes wrong:**
If the launcher checks the manifest, starts downloading changed files, and the server-side manifest/files get updated again (or files get replaced) while the download is in progress, the client can end up with a mix of old and new mod versions — a classic recipe for Forge crash-on-launch (missing dependency version, mismatched mod jar for a config) or, worse, a client that joins the server with an incompatible mod set and desyncs/crashes other players.

**Why it happens:**
A naive "read manifest → download changed files → done" flow has no atomicity: the manifest can be read at time T, then a file referenced in it gets overwritten with a newer version at T+1 before the download at T+2 fetches it, producing an internally inconsistent client (files from two different manifest generations).

**How to avoid:**
- Version/hash the manifest itself (e.g., a monotonic `manifest_version` or content hash) and have the launcher pin to a single manifest snapshot for the entire update run — never re-read the manifest mid-download.
- Publish updates atomically on the file server: build the full new file set in a staging directory, then atomically swap the "current" symlink/pointer only after all files are in place — never mutate files in-place under an already-published manifest.
- Verify every downloaded file's hash against the pinned manifest's expected hash before considering the update complete; on mismatch, retry that file against the (possibly now-newer) manifest rather than silently proceeding with a stale file.

**Warning signs:**
- Intermittent, hard-to-reproduce Forge crash reports that "fix themselves" on a second launch (classic symptom of a client that grabbed a half-updated file set once).
- Client and server mod-version logs disagree after a deploy that happened while someone was mid-download.

**Phase to address:**
File-server/manifest design phase — build atomicity in from the start; retrofitting it after players hit corrupted installs is much more painful.

---

### Pitfall 10: Home-network hosting assumptions break without warning (dynamic IP, CGNAT, router reboot)

**What goes wrong:**
Residential ISP connections typically hand out a dynamic IP that can change on router reboot or DHCP lease renewal — if the launcher/players are configured with a hardcoded IP, the server becomes unreachable with zero warning after any router hiccup or ISP-side maintenance. Worse, some residential ISPs use CGNAT (Carrier-Grade NAT), where the home router doesn't have a public IP at all — port forwarding silently does nothing because there is no public IP to forward to, and this can be true even when the user's own router config "looks correct."

**Why it happens:**
It's the default plan to just "port forward and give friends the IP" — this works until the first IP change, and doesn't work at all under CGNAT, which many users don't know they have until testing reveals inbound connections never arrive despite correct local port-forward rules.

**How to avoid:**
- First, confirm the ISP does NOT use CGNAT — check whether the router's WAN IP matches the public IP shown by an external "what's my IP" check; if they differ, it's CGNAT and port forwarding is a dead end (a tunneling/reverse-proxy service, e.g., a lightweight relay, becomes mandatory rather than optional).
- Set up Dynamic DNS (DDNS) from day one regardless of CGNAT status — a hostname (`rlcraft.duckdns.org`-style) that the launcher/server-list use instead of a raw IP, with an update client running on the network to keep the DNS record current after IP changes.
- Bake the DDNS hostname into the launcher's server connection default so a friend never has to be told "the IP changed, use this new one instead."
- Document the port-forward requirement (server's game port, typically 25565, forwarded UDP/TCP as required) as an explicit setup checklist item, tested by having a friend on a different network attempt to connect before the "it's live" milestone.

**Warning signs:**
- Server works for LAN-local testing but unreachable friends can't connect at all (possible CGNAT).
- Server "goes down" for everyone simultaneously with no crash in the logs — likely an IP change with no DDNS in place.

**Phase to address:**
Server networking/hosting phase — resolve CGNAT status and set up DDNS before the "server reachable via public IP/domain" requirement is considered met.

---

### Pitfall 11: No tested backup/restore process for a large modded world

**What goes wrong:**
Modded worlds (many dimensions, custom chunk data from world-gen mods) are large and slow to back up naively; a `cp -r` or zip while the server is live risks copying chunk files mid-write, producing a backup that looks complete but is actually corrupted (truncated/partial region files) — discovered only when you actually need to restore it, which is the worst possible time to find out.

**Why it happens:**
Backups get treated as a checkbox ("we have a cron job that zips the world folder") rather than a tested process; nobody restores from a backup until a real corruption event forces it, at which point a silently-broken backup chain is discovered.

**How to avoid:**
- Use `save-off` / `save-all` (or an equivalent Forge console command) to flush and pause world saving before copying files, then `save-on` after — never back up a live-writing world folder directly.
- Prefer full server stop for backups if the friend group's play schedule allows a short nightly maintenance window — the only backup approach guaranteed 100% consistent.
- Automate daily backups at minimum, with hourly for active dimensions if disk space allows; keep a rolling retention window (e.g., 7 daily + 4 weekly) rather than unbounded growth on a 133 GB disk shared with other services.
- Periodically (monthly at minimum) actually test-restore a backup to a scratch directory and load-check it — a backup that has never been restored is unverified, not a backup.
- Compress backups (typically 50-70% size reduction) to conserve the Pi's disk budget, and consider moving older backups off-Pi (external drive, cloud) for the "1 off-site copy" leg of a 3-2-1 strategy.

**Warning signs:**
- No documented/tested restore procedure exists.
- Backup job has never been manually verified by actually opening a restored world.
- Disk usage climbing unbounded from backup retention with no pruning.

**Phase to address:**
Server operations/reliability phase (autostart + backups requirement) — backups should be built and *test-restored* before going live with the friend group, not bolted on after a real corruption event.

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|-----------------|------------------|
| Hardcode server IP in launcher instead of DDNS hostname | Faster initial setup | Every IP change breaks all friends' launchers silently | Never — DDNS costs almost nothing to set up first |
| Password check only in launcher, not server | Faster to build (one auth surface) | Server is fully open to anyone with the IP and any Minecraft client | Never — this is a real security hole, not a cosmetic gap |
| Skip macOS code signing/notarization | Saves $99/yr + setup time | App literally will not open on macOS without manual Gatekeeper override each time | Only if every Mac friend is technical enough to run the terminal `xattr` workaround — otherwise blocks onboarding |
| Ship unsigned Windows exe | Saves cert cost | SmartScreen full-screen warning on every friend's first run | Acceptable for a 5-7 person closed group if documented clearly in onboarding — revisit if it causes repeated confusion |
| Mirror CurseForge mod jars on your own file server without checking each license | Simpler, single-source downloads, works if CurseForge API is briefly down | Legal/ToS risk per mod author's redistribution terms | Never for non-redistributable mods; fine for MIT/GPL-licensed or explicitly-allowed mods |
| Zip the live world folder without save-off | Simple cron one-liner | Silent chance of corrupted/unusable backup discovered only during a real restore | Never — the `save-off`/`save-all` wrapper is a few extra lines |
| Single full-heap `-Xmx` with no Aikar-style GC flags | Works fine at low player counts | GC pauses/stutter worsen non-linearly as heap and entity count grow on the Pi's limited cores | Only acceptable during early solo/dev testing, tune before onboarding the group |

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|-------------------|
| Mojang asset/client download | Bundling the Minecraft jar/assets on your own file server | Have the launcher fetch vanilla client jar + assets directly from Mojang's official version manifest/CDN, same as the official launcher does |
| CurseForge mods | Assuming all mods can be freely mirrored | Use the CurseForge API with the modpack's manifest.json (project/file IDs) to fetch mods respecting each author's distribution permission flag |
| Java runtime vendors | Assuming one vendor/URL works for all platforms | Use a per-platform vendor matrix: Zulu/Liberica for macOS ARM64 Java 8, Temurin (or same) fine for Windows x64/macOS Intel |
| Tauri updater/download events | Using the general event bus for high-frequency progress updates | Use Tauri's channel API for streaming download progress efficiently |
| DDNS provider | Setting DDNS once and never verifying the update client is still running | Monitor/alert if the DDNS record hasn't updated within an expected window (e.g., cron healthcheck) |

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|-----------------|
| Default RLCraft view-distance (10) on Pi 5 | "Can't keep up" ticks skipped, chunk-gen stalls | Lower view-distance to 6-8, tune mob caps, use a profiler (Spark) to find hot mods | Breaks almost immediately with 2+ players exploring simultaneously |
| Oversized `-Xmx` heap "just because RAM is available" | Longer GC pauses, not shorter | Right-size heap to RLCraft's actual working set (6-8 GB) with `-Xms` = `-Xmx`, use Aikar's flags tuned with higher `G1NewSizePercent` for modded workloads | Breaks once heap exceeds what G1GC can sweep within a tick budget — bigger isn't free |
| Running client under Rosetta 2 on Apple Silicon instead of native ARM64 LWJGL | Sub-30fps, "barely playable" reports | Bundle ARM64-native LWJGL 2 + jinput binaries, detect arch in launcher | Breaks visibly from first launch — not a scale issue, a correctness issue on that hardware |
| No entity/mob cap tuning for RLCraft's dense mob mods | Gradual TPS decay over hours of continuous world uptime as entities accumulate | Cap mob spawns per chunk/dimension via config, periodic entity-count monitoring | Breaks as a slow-burn issue after hours/days of uninterrupted server uptime, not immediately |
| Live world folder backup without save-off | Occasional corrupted region files in backups | `save-off`/`save-all` wrapper, ideally full stop for backup | Breaks unpredictably — depends on exact timing of the copy vs. active writes |

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| Auth enforced client/launcher-side only | Anyone with the IP + any Minecraft client can join as any username, impersonate players, grief the world | Enforce password/session check server-side (auth mod/plugin or custom login-gate logic), never trust the launcher alone |
| Plaintext or weakly-hashed passwords in the auth DB | Full credential compromise if the DB/file leaks | bcrypt (cost ≥12) or Argon2id with proper salting; never roll your own hashing |
| No rate-limiting on registration/login in the auth service | Trivial to brute-force weak passwords or spam-register accounts | Basic rate-limit/lockout on repeated failed attempts per IP/username |
| Exposing the file server's download HTTP endpoint with no integrity checking | A MITM or compromised file server could push malicious files to the launcher | Manifest-based hash verification of every downloaded file before use; consider HTTPS for the file server endpoint |
| Treating "friends only, closed group" as a reason to skip all of the above | A single leaked IP + weak/reused password is enough for an outsider to join and grief | Apply the same baseline security regardless of group size — cost to do it right is low, cost of a griefed world is high |

## UX Pitfalls

| Pitfall | User Impact | Better Approach |
|---------|-------------|-------------------|
| No download progress feedback during large mod/Java downloads | Friends assume the launcher is frozen/broken, force-quit mid-download, corrupt the install | Stream real progress via Tauri channels, show percentage + cancel option |
| Unsigned installer triggers scary OS warnings | Non-technical friends may refuse to proceed, think it's malware | Sign macOS build (cheap, essential); document the Windows SmartScreen click-through clearly for a small trusted group |
| Hardcoded server IP that changes | "It stopped working" reports with no clear cause | DDNS hostname baked into the launcher default from day one |
| RAM slider with no sane bounds/guidance | Friends set RAM too low (crashes) or too high (starves their own machine or exceeds installed RAM) | Auto-detect system RAM, suggest a safe default (matching the ~6-8 GB server-side target minus headroom), clamp the slider to sane bounds |
| Silent failure when Java 8 provisioning picks the wrong architecture build on Apple Silicon | Game runs (badly) under Rosetta with no explanation of why it's slow | Verify fetched JDK architecture matches host arch, surface a clear error/retry if mismatched |

## "Looks Done But Isn't" Checklist

- [ ] **Server-side auth:** Often "done" only in the launcher — verify a bare vanilla Minecraft client (no launcher) cannot join or move in the world without passing the same password check.
- [ ] **Java 8 provisioning on Apple Silicon:** Often "done" via a hardcoded Temurin URL that has no arm64 Java 8 build — verify the downloaded JDK reports `aarch64`/`arm64`, not `x86_64`, on an actual M-series Mac.
- [ ] **Backups:** Often "done" as a cron job that's never been restored — verify by actually restoring a backup to a scratch world and loading it.
- [ ] **DDNS:** Often "done" as a one-time router config — verify the update client is actually running continuously and the hostname reflects the current IP after a forced router reboot.
- [ ] **Modpack/mod licensing:** Often "done" by just mirroring the whole CurseForge modpack folder — verify each non-CurseForge-hosted or specially-licensed mod's redistribution permission before serving it from your own file server.
- [ ] **macOS code signing:** Often "done" as "we'll sign it eventually" — verify by testing the actual `.app` download+launch flow on a clean (non-dev) Mac account, where Gatekeeper behavior is strictest.
- [ ] **Update manifest atomicity:** Often "done" as a simple file-hash-diff downloader — verify behavior when the manifest changes mid-download (simulate a deploy while a client is mid-update).
- [ ] **TPS under real load:** Often "done" after a solo smoke test — verify with 2-3 simultaneous players actively exploring/fighting before calling server performance acceptable.

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|----------------|------------------|
| Discovered server-side auth gap after players onboarded | MEDIUM | Add server-side login gate, force all players to re-authenticate on next join, audit logs for any unauthorized joins in the gap window |
| Corrupted world from a bad backup restore | HIGH (if no verified backup exists) / LOW (if a verified good backup exists) | Restore from the most recent verified-good backup; if none exists, attempt region-file repair tools before declaring data loss |
| Discovered non-redistributable mod jars on the file server | LOW-MEDIUM | Remove the jars, switch that mod's delivery to CurseForge-API-direct fetch, audit the rest of the mod list for the same issue |
| Apple Silicon players stuck on Rosetta with bad performance | LOW-MEDIUM | Ship an updated launcher release bundling ARM64-native LWJGL 2 + jinput, push via the update mechanism |
| CGNAT discovered after "go live" | MEDIUM | Stand up a lightweight tunneling/reverse-proxy relay (e.g., a small always-on VPS or tunneling service) in front of the Pi, update DDNS/manifest to point at the relay |
| GC stutter/TPS issues discovered after onboarding | LOW | Apply Aikar-style flags + heap right-sizing, restart server during a scheduled maintenance window, communicate downtime in advance |

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|-------------------|----------------|
| ARM CPU/TPS bottleneck under RLCraft | Server setup / performance-tuning phase | Load test with 2-3 real players, TPS stays acceptable (target ≥18-19 TPS) over a 30+ min session |
| Java version mismatch (server & client) | Server bootstrap phase; Launcher Java-provisioning phase | Explicit Java-8-path pinning in start script; launcher verifies fetched JDK major version = 8 before first launch |
| Offline-mode auth trusted client-side only | Auth/security phase (parallel with server offline-mode setup) | Manual test: connect with a bare vanilla client, confirm rejection/kick without valid password |
| Mojang/mod redistribution legality | File-server/manifest design phase | Manifest excludes Minecraft jar/assets; each mirrored mod jar checked against CurseForge distribution permission |
| LWJGL 2 / Apple Silicon rendering | macOS launcher/client-bootstrap phase | Real hardware test on an M-series Mac: game launches, renders, no Rosetta fallback unless explicitly intended |
| Java 8 vendor availability on macOS ARM64 | Launcher Java-provisioning phase | Downloaded JDK's `java -version`/arch check confirms `aarch64` on Apple Silicon test machine |
| Unsigned binaries blocked by OS | Launcher packaging/distribution phase | macOS build signed+notarized and opens cleanly on a clean test account; Windows SmartScreen behavior documented in onboarding |
| No download progress feedback | Launcher download/update-manifest phase | UI shows live percentage/byte progress during a large (Java/modpack) download, cancel-safe |
| Update manifest race conditions | File-server/manifest design phase | Simulated test: trigger a manifest update mid-download, confirm client ends up fully consistent (not mixed-version) |
| Home-network hosting (dynamic IP/CGNAT) | Server networking/hosting phase | External-network friend successfully connects using the DDNS hostname after a forced router IP change |
| Untested world backups | Server operations/reliability phase | Backup successfully restored to a scratch directory and loaded before going live with the friend group |

## Sources

- [Raspberry Pi 5 Minecraft Server: Performance Guide - GameTeam](https://gameteam.io/blog/raspberry-pi-5-minecraft-server-performance-guide/)
- [RLCraft Server Requirements: RAM & Performance Guide - GameTeam](https://gameteam.io/blog/rlcraft-server-requirements-ram-performance-guide/)
- [How to fix RLCraft Lag on your Minecraft server - StickyPiston](https://stickypiston.co/account/knowledgebase/143/RLCraft-Lag-and-Issues-Server-and-Player-Solutions-Explained.html)
- [Rl craft server constantly lagging - Admincraft](https://www.answeroverflow.com/m/1125688056259825684)
- [Failed to run Minecraft Forge server for 1.12.2 · Issue #7596 - MinecraftForge/MinecraftForge](https://github.com/MinecraftForge/MinecraftForge/issues/7596)
- [Starting forge 1.12.2 with java 11 [Solved] - Forge Forums](https://forums.minecraftforge.net/topic/73597-starting-forge-1122-with-java-11-solved/)
- [Aikar's flags | PaperMC Docs](https://docs.papermc.io/paper/aikars-flags/)
- [Minecraft Server RAM - Right Size in 2026 - MineStrator](https://minestrator.com/blog/post/minecraft-server-ram-2026-vanilla-paper-modded-gb)
- [Griefing-Methods/Exploitation/UUID Spoofing.md - wodxgod](https://github.com/wodxgod/Griefing-Methods/blob/master/Exploitation/UUID%20Spoofing.md)
- [Why you shouldn't use Offline Mode on a Minecraft Server](https://madelinemiller.dev/blog/minecraft-offline-mode/)
- [Exporting a Modpack for CurseForge project submission - CurseForge Support](https://support.curseforge.com/support/solutions/articles/9000198500-exporting-a-modpack-for-curseforge-project-submission)
- [Modpack Rules - reikakalseki.github.io](https://reikakalseki.github.io/minecraft/pages/perms/packrules.html)
- [CurseForge FAQ | Safety, Legality & Mod Manager Questions](https://curseforge.dev/faq.html)
- [Provide JDK/JRE 8 and 11 builds for Apple Silicon · Issue #96 - adoptium/adoptium](https://github.com/adoptium/adoptium/issues/96)
- [Java on Apple Silicon or Intel Macs · Mac Install Guide 2026](https://mac.install.guide/java/apple-silicon)
- [Run LWJGL 2 Natively on Apple Silicon - shadowfacts.net](https://shadowfacts.net/2022/lwjgl-arm64/)
- [Minecraft with Forge crashes before showing on Apple Silicon M1 - Forge Forums](https://forums.minecraftforge.net/topic/106576-minecraft-with-forge-crashes-before-showing-on-apple-silicon-m1/)
- [[bug] MSI and <app>.exe false positives on anti-virus apps · Issue #4749 - tauri-apps/tauri](https://github.com/tauri-apps/tauri/issues/4749)
- [Why do I get this prompt every time I install on Windows? · Discussion #8046 - tauri-apps/tauri](https://github.com/tauri-apps/tauri/discussions/8046)
- [Windows Code Signing | Tauri Docs - techXcelerate](https://techxcelerate.ntxm.org/docs/tauri/building--distribution/code-signing/windows-code-signing/)
- [Updater | Tauri v2](https://v2.tauri.app/plugin/updater/)
- [How to get the progress of tauri http upload or download file? · Discussion #4726 - tauri-apps/tauri](https://github.com/tauri-apps/tauri/discussions/4726)
- [Living with(out) notarization - The Eclectic Light Company](https://eclecticlight.co/2024/10/01/living-without-notarization/)
- [Your Mac App Is Not Broken: Gatekeeper May Just Distrust an Unsigned Tool - Margrop Blog](https://blog.margrop.net/en/post/macos-gatekeeper-unsigned-app-fix/)
- [Code Signing a Tauri App for macOS — The Complete Flow - DEV Community](https://dev.to/hiyoyok/code-signing-a-tauri-app-for-macos-the-complete-flow-54jk)
- [Hosting a Minecraft Server Behind CGNAT - LinkedIn](https://www.linkedin.com/pulse/hosting-minecraft-server-behind-cgnat-jeffrey-samuels)
- [How to Set Up Dynamic DNS for Your Minecraft Server - Astroworld Guides](https://guide.astroworldmc.com/how-to-set-up-dynamic-dns-minecraft-server)
- [How to Host a Minecraft Server at Home Without Port Forwarding - Localtonet](https://localtonet.com/blog/how-to-host-a-minecraft-server-at-home-without-port-forwarding)
- [The Only Guide You'll Ever Need for Minecraft Server Backup and Recovery - Host Havoc](https://hosthavoc.com/blog/minecraft-server-backup-recovery)
- [Minecraft Server World Corruption: Backup & Recovery Guide - GameTeam](https://gameteam.io/blog/minecraft-server-world-corruption-backup-recovery-guide/)
- [Overcoming the Lag: A Deep Dive into RLCraft Chunk Loading](https://ftp.survation.com/overcoming-the-lag-a-deep-dive-into-rlcraft-chunk-loading/)
- [Forge Crashes when Launched with Mixin Bootstrap - Forge Forums](https://forums.minecraftforge.net/topic/95396-forge-crashes-when-launched-with-mixin-bootstrap/)
- [Crash With MixinBootstrap mod (1.12.2) · Issue #316 - DimensionalDevelopment/VanillaFix](https://github.com/DimensionalDevelopment/VanillaFix/issues/316)
- [Picking a password hash: A developer's guide to argon2, bcrypt, and scrypt - WorkOS](https://workos.com/blog/picking-a-password-hash-argon2-bcrypt-scrypt)
- [Password Hashing in 2026: bcrypt vs Argon2 vs scrypt vs PBKDF2 - toolsana.com](https://toolsana.com/blog/password-hashing-2026-bcrypt-argon2-scrypt-pbkdf2-guide/)
- [Minecraft-on-Apple-Silicon-Benchmark-WITHOUT-Rosetta-2 - honeyvig/GitHub](https://github.com/honeyvig/Minecraft-on-Apple-Silicon-Benchmark-WITHOUT-Rosetta-2-Apple-M1-Apple-M1-Max-/blob/main/README.md)

---
*Pitfalls research for: RLCraft (Forge 1.12.2) Pi 5 server + offline-mode auth + Tauri launcher*
*Researched: 2026-08-27*
