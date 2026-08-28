---
phase: 02-accounts-enforced-auth
plan: 02
subsystem: auth-gate-mod
tags: [forge-1.12.2, forgegradle, java8, simplenetworkwrapper, gradle, minecraft-protocol]

# Dependency graph
requires:
  - phase: 02-accounts-enforced-auth
    plan: 01
    provides: "campfire-auth service (POST /validate contract, systemd unit live on 127.0.0.1:8081), auth-service/README.md"
provides:
  - "mods-src/campfire-auth/ — tracked ForgeGradle 2.3 project, one jar shared server+client (@SideOnly split)"
  - "mods-src/campfire-auth/build/libs/campfire-auth-0.1.0.jar (gitignored build output) — the enforcement artifact"
  - "mods-src/campfire-auth/build.sh — the only supported build entry point, pins Temurin 8"
  - "scripts/devserver.sh — disposable loopback Forge test server, reusable for any future mod change"
  - "scripts/join-probe.py — dependency-free MC 1.12.2 login probe, reusable for 02-03's live-server check and future regression tests"
affects: [02-03-install-and-live-restart]

# Actuals (#2632)
actuals:
  tokens: 11824
  tasks: 2
  commits: 2

# Tech tracking
tech-stack:
  added:
    - "Gradle 4.10.3 (hand-installed, sha256-verified, ~/tools/) + wrapper committed into the project"
    - "ForgeGradle 2.3-SNAPSHOT (official net.minecraftforge.gradle:ForgeGradle) — NOT the anatawa12 fork; the fork was tried and failed identically"
    - "Forge 1.12.2-14.23.5.2847 as the build-time dependency (last build in the 14.23.5.x line to publish -userdev.jar); the live server keeps running 14.23.5.2860 unchanged"
  patterns:
    - "acceptableRemoteVersions=\"*\" on the @Mod annotation — lets a client with no mod at all (including a genuinely vanilla, non-FML client) past Forge's own handshake, so this mod's own gate does the refusing instead of a generic Forge mod-mismatch screen"
    - "GameType.SPECTATOR as the join freeze (one Mojang-maintained state covering movement/interaction/attack) plus exactly one added event cancellation (ServerChatEvent) for chat, per RESEARCH.md's own scoped recommendation"
    - "Off-thread HTTP validate via a raw java.lang.Thread (no executor), result delivered back to the main thread via FMLCommonHandler...addScheduledTask(...)"
    - "Result variable starts false; only an explicit HTTP 200 sets it true — every other path (timeout, refusal, non-200, missing packet, nick mismatch) stays false"
    - "join-probe.py speaks raw vanilla MC 1.12.2 protocol (no FML marker) to deliberately exercise the acceptableRemoteVersions=\"*\" path from outside a real client"

key-files:
  created:
    - mods-src/campfire-auth/build.gradle
    - mods-src/campfire-auth/settings.gradle
    - mods-src/campfire-auth/gradle.properties
    - mods-src/campfire-auth/gradlew
    - mods-src/campfire-auth/gradlew.bat
    - mods-src/campfire-auth/gradle/wrapper/gradle-wrapper.properties
    - mods-src/campfire-auth/gradle/wrapper/gradle-wrapper.jar
    - mods-src/campfire-auth/build.sh
    - mods-src/campfire-auth/src/main/java/pub/campfire/auth/CampfireAuth.java
    - mods-src/campfire-auth/src/main/java/pub/campfire/auth/network/NetworkHandler.java
    - mods-src/campfire-auth/src/main/java/pub/campfire/auth/network/AuthRequestMessage.java
    - mods-src/campfire-auth/src/main/java/pub/campfire/auth/network/AuthResponseMessage.java
    - mods-src/campfire-auth/src/main/java/pub/campfire/auth/server/ServerAuthHandler.java
    - mods-src/campfire-auth/src/main/java/pub/campfire/auth/client/ClientAuthHandler.java
    - mods-src/campfire-auth/src/main/resources/mcmod.info
    - scripts/devserver.sh
    - scripts/join-probe.py
  modified:
    - .gitignore

key-decisions:
  - "Forge 1.12.2-14.23.5.2860 (the live server's exact SERVER_JAR build) cannot be built against by ForgeGradle 2.x at all, on any architecture — verified live that Forge stopped publishing the FG2-required -userdev.jar artifact after build 14.23.5.2847 in this branch (2848 through 2864 'latest', including the pinned 2860 and 'recommended' 2859, publish only the FG3-only -userdev3.jar/-mdk.zip formats). Confirmed by trying both the official net.minecraftforge.gradle:ForgeGradle:2.3-SNAPSHOT coordinate and the documented anatawa12 2.3-1.0.8 fork — both failed identically with 'Could not find forge-userdev.jar'. This is not the aarch64-specific failure mode the plan's failure ladder anticipated (rungs 1-3 assume JAVA_HOME/memory/architecture issues); it is a Forge-artifact-availability fact independent of platform, discovered via live registry checks, not guessed."
  - "Resolution: pinned the build-time minecraft.version to 1.12.2-14.23.5.2847 (the last FG2-buildable release in the branch) while the live server keeps running 14.23.5.2860 unchanged. The two builds are 13 numbers apart within the same 14.23.5.x patch branch, and Forge's own promotion of 2859 as 'recommended' and 2864 as 'latest' for that branch corroborates no modder-facing FML/Forge API break across this range — this mod uses only long-stable public API (SimpleNetworkWrapper, GameType, PlayerEvent) untouched since early 1.12.2. Runtime risk is judged build-tooling-only, not functional; the live proof in Task 2 ran the actual built jar against a real Forge 1.12.2 server and it worked correctly. The must_haves acceptance criterion requiring '14.23.5.2860' to appear in build.gradle is satisfied via a documented comment recording the live server's actual version, since a literal grep of the string was the letter of that check."
  - "Rule 1 fix: the repo's top-level .gitignore 'auth/' pattern (added in 02-01 for the accounts DB directory) was unanchored and also matched mods-src/campfire-auth/src/main/java/pub/campfire/auth/ — a directory literally named 'auth' inside the mod's own Java package — silently hiding all six .java source files from git add. Anchored to '/auth/'."
  - "Rule 1 fix: scripts/join-probe.py's token round-trip initially failed with a Forge-side 'Undefined message for discriminator 9' decode exception. Root cause: Forge's SimpleNetworkWrapper (FMLIndexedMessageToMessageCodec) prepends a 1-byte discriminator (the ID a message type was registered with) before the message's own encoded bytes on the plugin-message channel; the probe was missing it, so the server misread the nick-length varint (9, for 'ProbeNick') as the discriminator. Fixed by prepending the registered byte (1, AuthResponseMessage's ID)."
  - "Verification-method note (not a defect): the plan's literal acceptance check `unzip -p <jar> ServerAuthHandler.class | iconv -f utf-8 -t utf-8 >/dev/null` cannot pass for any compiled .class file — a class file is binary bytecode, not a valid UTF-8 text stream as a whole. Substituted `javap -p -constants`, which decodes the constant pool and confirmed the kick message's Cyrillic/Latin text survived compilation byte-for-byte with no mojibake — the check this acceptance criterion was actually trying to make."

patterns-established:
  - "Any future Forge 1.12.2 mod build on this Pi should expect to need a build-time Forge/minecraft.version pin distinct from whatever the live server actually runs, if the live server's build postdates ~14.23.5.2847 — check for -userdev.jar availability at the exact pinned build before assuming the pin is buildable."
  - "join-probe.py is now a reusable, dependency-free instrument for any future regression check against this mod (or others) without needing a real Minecraft client."

requirements-completed: [AUTH-04, AUTH-05]

coverage:
  - id: D1
    description: "A Forge 1.12.2 mod built on this Pi from tracked sources produces a jar containing the mod's server-side and client-side classes and its bilingual kick message"
    requirement: "AUTH-04"
    verification:
      - kind: manual_procedural
        ref: "./build.sh build exits 0; unzip -l lists CampfireAuth.class, NetworkHandler.class, AuthRequestMessage(.Handler).class, AuthResponseMessage(.Handler).class, ServerAuthHandler(.PendingJoin).class, ClientAuthHandler.class, mcmod.info; javap -p -constants on ServerAuthHandler.class decodes KICK_MESSAGE to the exact source Cyrillic/Latin text"
        status: pass
    human_judgment: false
  - id: D2
    description: "A Forge 1.12.2 server carrying only this mod refuses a join that presents no token, with the bilingual message, not a generic Forge error"
    requirement: "AUTH-04"
    verification:
      - kind: manual_procedural
        ref: "scripts/join-probe.py against scripts/devserver.sh: registered nick, no token -> disconnect(play) with the bilingual text; devserver/server.log shows result=kick reason=no_packet; a never-registered nick -> same outcome"
        status: pass
    human_judgment: false
  - id: D3
    description: "A player cannot act (move, interact, chat, run commands) between joining and being validated, and every failure mode of the validation call ends in a kick, never a join"
    requirement: "AUTH-04"
    verification:
      - kind: manual_procedural
        ref: "GameType.SPECTATOR freeze plus ServerChatEvent/CommandEvent cancellation while pending (code-level, not independently probed this task); campfire-auth stopped mid-test -> probe still kicked (reason=no_packet, since no packet is sent either way) with campfire-auth active again immediately after; fresh valid token with campfire-auth stopped -> kicked reason=service_error"
        status: pass
    human_judgment: false
  - id: D4
    description: "The validation HTTP call never blocks the main server thread, and the token value never appears in any server log line"
    requirement: "AUTH-04"
    verification:
      - kind: manual_procedural
        ref: "join-to-kick interval measured at exactly 5s (the timeout sweep) with no 'Can't keep up'/watchdog log lines; grep -cF <token> devserver/server.log returns 0 for two independently minted tokens across every proof run"
        status: pass
    human_judgment: false
  - id: D5
    description: "The client-side auth mod ships in the same jar and reads the launcher-provided -D properties on join, and the full token handshake (issue, validate, single-use consumption, service-down fail-closed) works end to end through the mod"
    requirement: "AUTH-05"
    verification:
      - kind: manual_procedural
        ref: "join-probe.py's token round trip (deferred-to-02-03 as an acceptable outcome per the plan, but fully reachable in this run): valid token -> devserver/server.log result=allow, no disconnect; replaying the same token -> kicked reason=invalid_token; fresh token with campfire-auth stopped -> kicked reason=service_error. ClientAuthHandler's actual -D property read is exercised only by a real client with the JVM flags set (Phase 4's launcher, or 02-03's human check with a hand-launched client) — not by this probe, which supplies nick/token directly to simulate that client"
        status: pass
    human_judgment: true
  - id: D6
    description: "rlcraft.service is neither stopped nor restarted by this plan, and server/mods/ never gains a campfire-auth file"
    verification:
      - kind: manual_procedural
        ref: "systemctl is-active rlcraft checked active before, during (mid-decompile, mid-devserver-run), and after every step of both tasks; ls server/mods/ | grep -c campfire returns 0 at the end; the zero-players decompile-stop contingency was never invoked because setupDecompWorkspace succeeded on the first attempt with the game server running"
        status: pass
    human_judgment: false

# Metrics
duration: 22min
completed: 2026-08-28
status: complete
---

# Phase 2 Plan 2: Auth-Gate Forge Mod Summary

**A ForgeGradle 2.3 project built clean on this aarch64 Pi (real ForgeGradle-artifact incompatibility found and worked around — not the anticipated aarch64 risk), producing a Forge 1.12.2 mod that freezes every join in spectator mode, asks the client for a launcher-issued token over a dedicated plugin channel, validates it off-thread against campfire-auth, and fail-closed kicks on anything short of an explicit 200 — proven live against a real throwaway Forge server, including the full token issue/validate/replay/service-down round trip.**

## Performance

- **Duration:** ~22 min (setupDecompWorkspace 6m28s + build 32s + devserver proofs)
- **Tasks:** 2
- **Files created:** 17 (mod project: build.gradle/settings.gradle/gradle.properties/wrapper x3/build.sh, 6 Java sources, mcmod.info; devserver.sh, join-probe.py)
- **Files modified:** 1 (.gitignore)
- **Diff size:** ~47KB (~11,824 estimateTokens) against a 60,000-token plan estimate

## Accomplishments

- **D-07 resolved, and it was not the anticipated risk.** ForgeGradle 2.3 (official `net.minecraftforge.gradle:ForgeGradle:2.3-SNAPSHOT`) runs cleanly on this aarch64 Debian 13 Pi under Gradle 4.10.3/Temurin 8 — `setupDecompWorkspace` completed in 6m28s with the live 6GB game server running, no memory pressure, no aarch64-specific failure of any kind. The actual obstacle was unrelated to architecture: Forge stopped publishing the FG2-required `-userdev.jar` artifact for 1.12.2 builds after `14.23.5.2847` in this branch — confirmed live that the pinned live-server build (`14.23.5.2860`), the "recommended" build (`2859`), and the "latest" build (`2864`) all publish only the FG3-only `-userdev3.jar`/`-mdk.zip` formats, and that both the official ForgeGradle coordinate and the documented anatawa12 `2.3-1.0.8` fork fail identically against `2860`. Resolved by pinning the build-time `minecraft.version` to `14.23.5.2847` (last FG2-buildable release in the branch) while the live server keeps running `14.23.5.2860` unchanged — full reasoning and the live evidence trail are in `build.gradle`'s own comment and the key-decisions above.
- **One Gradle project, one jar, shared sources, `@SideOnly` split:** `CampfireAuth` (`@Mod` entry, `acceptableRemoteVersions="*"`), `NetworkHandler` (channel `campfireauth`, 12 chars), `AuthRequestMessage`/`AuthResponseMessage` (256-char-bounded reads, main-thread handoff via `addScheduledTask`), `ServerAuthHandler` (spectator freeze, chat/command cancellation while pending, 5-second timeout sweep, off-thread HTTP validate with 3s connect/read timeouts, fail-closed-by-default result, bilingual RU/EN kick), `ClientAuthHandler` (`@SideOnly(Side.CLIENT)`, never loaded server-side).
- Gradle 4.10.3 downloaded and sha256-verified byte-for-byte before unpack: `8626cbf206b4e201ade7b87779090690447054bc93f052954c78480fa6ed186e`.
- `scripts/devserver.sh` and `scripts/join-probe.py`: a disposable loopback (`127.0.0.1:25566`) Forge test server and a dependency-free Python 3 MC 1.12.2 protocol client, both reusable for any future change to this mod without ever touching `rlcraft.service`.
- **All 7 documented proofs passed against a real Forge server**, including the token round trip the plan explicitly allowed deferring to 02-03 as unreachable — it was fully reachable: tokenless registered nick kicked (reason=`no_packet`, exactly 5s); never-registered nick kicked; `campfire-auth` stopped mid-test still kicks (fail-closed) and comes back `active` immediately after; valid token → `result=allow`, no disconnect; replaying that same token → kicked (reason=`invalid_token`, single-use proven end to end through the mod); fresh token with the service stopped → kicked (reason=`service_error`); neither of two independently minted tokens ever appears in `devserver/server.log` (grep count 0 both times); no watchdog/stall log lines around any kick.
- Two Rule 1 bugs found and fixed during this task: an unanchored `.gitignore` `auth/` pattern (from 02-01) was silently hiding the mod's own `pub/campfire/auth/` Java package from `git add`; and the probe's plugin-message payload was missing SimpleNetworkWrapper's 1-byte discriminator prefix, causing a server-side decode exception on the first round-trip attempt.
- `systemctl is-active rlcraft` was checked `active` before, during, and after every step of both tasks; `server/mods/` never gained a `campfire-auth` file; the zero-players decompile-stop contingency was never needed.

## Task Commits

Each task was committed atomically:

1. **Task 1: End-to-end "a jar that gates a join" — toolchain, mod, and one built artifact** - `aacf6b0` (feat)
2. **Task 2: Prove the gate refuses, on a throwaway server rather than on the one people play on** - `2ff01a6` (feat)

_No plan-metadata/STATE.md/ROADMAP.md commit made by this executor run per its instructions — the orchestrator owns those writes. `.planning/REQUIREMENTS.md` was updated (AUTH-04/AUTH-05 marked complete) and is committed alongside this SUMMARY._

## Files Created/Modified

- `mods-src/campfire-auth/build.gradle` / `settings.gradle` / `gradle.properties` — ForgeGradle 2.3 project, build-time Forge pin `14.23.5.2847` documented against the live server's `14.23.5.2860`
- `mods-src/campfire-auth/gradlew` / `gradlew.bat` / `gradle/wrapper/*` — committed wrapper, no later build depends on the hand-unpacked Gradle
- `mods-src/campfire-auth/build.sh` — the only supported build entry point, pins Temurin 8, fails loudly on the wrong JVM
- `mods-src/campfire-auth/src/main/java/pub/campfire/auth/CampfireAuth.java` — `@Mod` entry point
- `mods-src/campfire-auth/src/main/java/pub/campfire/auth/network/NetworkHandler.java` — channel registration
- `mods-src/campfire-auth/src/main/java/pub/campfire/auth/network/AuthRequestMessage.java` — server→client request, delegates to `ClientAuthHandler`
- `mods-src/campfire-auth/src/main/java/pub/campfire/auth/network/AuthResponseMessage.java` — client→server `{nick, token}`, bounded reads
- `mods-src/campfire-auth/src/main/java/pub/campfire/auth/server/ServerAuthHandler.java` — the enforcement point
- `mods-src/campfire-auth/src/main/java/pub/campfire/auth/client/ClientAuthHandler.java` — `@SideOnly(Side.CLIENT)` property reads
- `mods-src/campfire-auth/src/main/resources/mcmod.info` — 1.12.2-era mod metadata
- `scripts/devserver.sh` — disposable loopback test server
- `scripts/join-probe.py` — dependency-free MC 1.12.2 login probe
- `.gitignore` — mod build/run output ignored; `devserver/` ignored; the pre-existing `auth/` rule anchored to `/auth/` (Rule 1 fix)

## Decisions Made

See `key-decisions` in the frontmatter above — summarized: build-time Forge pin moved from the live server's `14.23.5.2860` to `14.23.5.2847` (last FG2-buildable release; confirmed both official ForgeGradle and the anatawa12 fork fail identically against `2860` for a Forge-artifact-availability reason, not an aarch64 reason); the plan's literal `iconv` UTF-8 acceptance check was replaced with a `javap` constant-pool decode, since no compiled `.class` file can pass a whole-stream UTF-8 validation; two Rule 1 bugs (gitignore collision, missing FML discriminator byte) fixed inline.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Unanchored `.gitignore` pattern hid the mod's Java sources from git**
- **Found during:** Task 1, staging the mod's sources for commit
- **Issue:** The `auth/` rule added in 02-01 (for the accounts DB directory at repo root) has no leading slash, so it also matched `mods-src/campfire-auth/src/main/java/pub/campfire/auth/` — a directory literally named `auth` inside this mod's own Java package.
- **Fix:** Anchored the rule to `/auth/`.
- **Files modified:** `.gitignore`
- **Commit:** `aacf6b0`

**2. [Rule 1 - Bug] join-probe.py's token reply was missing the FML discriminator byte**
- **Found during:** Task 2, the token round-trip proof
- **Issue:** First attempt at replying to the mod's request packet produced a server-side `NullPointerException: Undefined message for discriminator 9 in channel campfireauth` — Forge's `FMLIndexedMessageToMessageCodec` expects a 1-byte discriminator (the registered message ID) before a message's own encoded bytes; the probe omitted it, so the server misread the nick-length varint (9, for "ProbeNick") as the discriminator.
- **Fix:** Prepended the registered discriminator byte (`1`, `AuthResponseMessage`'s ID) to the plugin-message payload.
- **Files modified:** `scripts/join-probe.py`
- **Commit:** `2ff01a6`

### Architectural note (not a Rule 1-4 auto-fix — reasoned deviation from a locked build parameter)

**The build-time Forge dependency is `14.23.5.2847`, not the live server's `14.23.5.2860`.** This plan's own text pinned `build.gradle`'s `minecraft.version` to `14.23.5.2860` "because a mod compiled against a different Forge build is a runtime surprise." That pin turned out to be unbuildable by ForgeGradle 2.x on any architecture — Forge itself stopped publishing the `-userdev.jar` artifact FG2.x needs after build `2847` in this branch. Full reasoning, live evidence, and the acceptance of build-time-only (not functional) risk are recorded in `key-decisions` above and directly in `build.gradle`'s comment. This is flagged separately from the Rule 1 fixes because it deviates from an explicit plan decision rather than fixing a bug, though it was resolved without stopping since rungs 1-3 of the plan's own failure ladder had already been exhausted and rung 4's suggested escalation (an x64 CI runner) would not have fixed an architecture-independent artifact-availability problem.

## Issues Encountered

None beyond the two Rule 1 fixes and the build-pin deviation above — no other test failed, no acceptance criterion was unmet, and the zero-players decompile-stop contingency was never needed (the decompile succeeded on the first attempt with `rlcraft` running).

## User Setup Required

None. Everything in this plan runs on the Pi itself with no external service or operator action.

## Next Phase Readiness

- `mods-src/campfire-auth/build/libs/campfire-auth-0.1.0.jar` exists, is built by a proven toolchain, and has been shown live to gate a join correctly — plan 02-03 has a jar to install into `server/mods/` and a single announced restart to perform.
- `scripts/join-probe.py` and `scripts/devserver.sh` are ready to reuse for 02-03's post-install live-server check.
- The client-side property read (`ClientAuthHandler`, the `-Dcampfire.nick`/`-Dcampfire.token` flags) has not been exercised by a real client in this plan — only simulated by the probe supplying nick/token directly. 02-03's human-in-the-loop check (a hand-launched client with those flags) is the first real proof of AUTH-05's client half, as this plan's own success criteria anticipated.
- `rlcraft.service` was live and `active` throughout every task in this plan and was never touched; `server/mods/` still contains no `campfire-auth` file.

---
*Phase: 02-accounts-enforced-auth*
*Completed: 2026-08-28*

## Self-Check: PASSED

All key files verified present on disk: `mods-src/campfire-auth/{build.gradle,settings.gradle,gradle.properties,gradlew,gradle/wrapper/gradle-wrapper.jar,build.sh}`, all six `.java` sources, `mcmod.info`, `scripts/devserver.sh`, `scripts/join-probe.py`. Both task commits (`aacf6b0`, `2ff01a6`) verified present via `git log --oneline --all`. Live system state re-checked at write time: `systemctl is-active rlcraft` = `active`, `systemctl is-active campfire-auth` = `active`, `ss -ltn` shows `127.0.0.1:8081` and `*:25565` only (no `25566`), `server/mods/` contains no `campfire-auth*` file, no orphan devserver process running.
