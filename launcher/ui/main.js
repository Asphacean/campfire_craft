// The whole single-screen frontend: form state, button handlers, status
// polling, the RAM slider, the Play sequence and its progress channel,
// Verify files, Game folder, Open log, and the update dialog (wave 4 adds
// the update dialog in a later task). No bundler and no ES module syntax
// anywhere — everything reaches Rust through window.__TAURI__.core.invoke
// (app.withGlobalTauri in tauri.conf.json).

const invoke = window.__TAURI__.core.invoke;
const Channel = window.__TAURI__.core.Channel;

const el = {
  statusPill: document.getElementById("status-pill"),
  statusText: document.querySelector("#status-pill .status-text"),
  errorBanner: document.getElementById("error-banner"),
  errorText: document.getElementById("error-text"),
  openLogBtn: document.getElementById("open-log-btn"),
  form: document.getElementById("auth-form"),
  nickInput: document.getElementById("nick-input"),
  passwordInput: document.getElementById("password-input"),
  loginBtn: document.getElementById("login-btn"),
  registerBtn: document.getElementById("register-btn"),
  sessionLine: document.getElementById("session-line"),
  sessionNick: document.getElementById("session-nick"),
  logoutBtn: document.getElementById("logout-btn"),
  ramBlock: document.getElementById("ram-block"),
  ramSlider: document.getElementById("ram-slider"),
  ramValue: document.getElementById("ram-value"),
  ramWarning: document.getElementById("ram-warning"),
  progressArea: document.getElementById("progress-area"),
  stepLabel: document.getElementById("step-label"),
  progressBar: document.getElementById("progress-bar"),
  infoBanner: document.getElementById("info-banner"),
  playBtn: document.getElementById("play-btn"),
  secondaryButtons: document.getElementById("secondary-buttons"),
  gameFolderBtn: document.getElementById("game-folder-btn"),
  verifyFilesBtn: document.getElementById("verify-files-btn"),
  versionFooter: document.getElementById("version-footer"),
};

let STRINGS = {};
let currentNick = null;
let systemMemory = null;
let infoDismissTimer = null;

function applyStaticCopy() {
  el.loginBtn.textContent = STRINGS.ctaLogin;
  el.registerBtn.textContent = STRINGS.ctaRegister;
  el.playBtn.textContent = STRINGS.ctaPlay;
  el.gameFolderBtn.textContent = STRINGS.btnGameFolder;
  el.verifyFilesBtn.textContent = STRINGS.btnVerifyFiles;
}

function showError(message) {
  el.errorText.textContent = message;
  el.errorBanner.hidden = false;
}

function clearError() {
  el.errorBanner.hidden = true;
  el.errorText.textContent = "";
}

// The file-repair message (D-08): informational, not alarming, auto-
// dismisses after about four seconds — reuses no state from the error
// banner at all, so a real error can never be silently swallowed by a
// stale dismiss timer.
function showInfo(message) {
  el.infoBanner.textContent = message;
  el.infoBanner.hidden = false;
  clearTimeout(infoDismissTimer);
  infoDismissTimer = setTimeout(() => {
    el.infoBanner.hidden = true;
  }, 4000);
}

function mapErrorCode(code) {
  switch (code) {
    case "invalid_credentials":
      return STRINGS.errorWrongPassword;
    case "network":
      return STRINGS.errorServerUnreachable;
    case "invalid_token":
    case "no_stored_session":
      return STRINGS.errorSessionExpired;
    case "nick_taken":
      return "That nickname is already taken.";
    case "invalid_nick":
      return "Nicknames are 3-16 letters, numbers, or underscores.";
    case "weak_password":
      return "Passwords must be at least 8 characters.";
    case "rate_limited":
      return "Too many attempts — try again later.";
    default:
      return STRINGS.errorServerUnreachable;
  }
}

// The Play/Verify sequence's own stable codes (`campfire_launcher_core
// ::play::PlayError::code`) — a distinct, smaller vocabulary from the auth
// commands' codes above; every sentence still comes from `strings.rs`.
function mapPlayErrorCode(code) {
  switch (code) {
    case "wrong_password":
      return STRINGS.errorWrongPassword;
    case "server_unreachable":
      return STRINGS.errorServerUnreachable;
    case "java_error":
      return STRINGS.errorJavaDownloadFailed;
    case "disk_full":
      return STRINGS.errorDiskFull;
    case "session_expired":
      return STRINGS.errorSessionExpired;
    default:
      return STRINGS.errorGeneric;
  }
}

function setFormBusy(busy, loadingLabel, activeBtn) {
  el.loginBtn.disabled = busy;
  el.registerBtn.disabled = busy;
  if (busy) {
    activeBtn.dataset.originalLabel = activeBtn.textContent;
    activeBtn.textContent = loadingLabel;
  } else {
    el.loginBtn.textContent = STRINGS.ctaLogin;
    el.registerBtn.textContent = STRINGS.ctaRegister;
  }
}

// D-06: the slider's value display and the >70%-of-physical-RAM warning —
// a sentence, never a blocking dialog; Play still works at that setting.
function updateRamDisplay() {
  const value = parseFloat(el.ramSlider.value);
  el.ramValue.textContent = `${value} GB`;
  const warn = systemMemory != null && value > systemMemory.total_gb * 0.7;
  el.ramSlider.dataset.warn = warn ? "true" : "false";
  if (warn) {
    el.ramWarning.textContent = STRINGS.ramWarning;
    el.ramWarning.hidden = false;
  } else {
    el.ramWarning.hidden = true;
  }
}

el.ramSlider.addEventListener("input", updateRamDisplay);

function showLoggedIn(nick) {
  currentNick = nick;
  el.form.hidden = true;
  el.sessionLine.hidden = false;
  el.sessionNick.textContent = nick;
  el.ramBlock.hidden = false;
  el.playBtn.hidden = false;
  el.secondaryButtons.hidden = false;
  updateRamDisplay();
}

function showForm(prefillNick) {
  currentNick = null;
  el.sessionLine.hidden = true;
  el.form.hidden = false;
  el.ramBlock.hidden = true;
  el.playBtn.hidden = true;
  el.secondaryButtons.hidden = true;
  el.progressArea.hidden = true;
  if (prefillNick) {
    el.nickInput.value = prefillNick;
  }
}

async function pollStatus() {
  try {
    const status = await invoke("get_status");
    if (status.online) {
      el.statusPill.dataset.state = "online";
      const counts = status.players != null && status.max != null ? ` · ${status.players}/${status.max}` : "";
      el.statusText.textContent = `${STRINGS.statusOnline}${counts}`;
    } else {
      el.statusPill.dataset.state = "offline";
      el.statusText.textContent = STRINGS.statusOffline;
    }
  } catch {
    el.statusPill.dataset.state = "offline";
    el.statusText.textContent = STRINGS.statusOffline;
  }
}

async function tryRestoreSession() {
  try {
    const session = await invoke("restore_session");
    showLoggedIn(session.nick);
  } catch (err) {
    const [code, nick] = String(err).split("|");
    if (code === "no_stored_session") {
      // Normal cold start — not an error, just show the empty form.
      showForm(undefined);
      return;
    }
    showForm(nick);
    showError(mapErrorCode(code));
  }
}

el.form.addEventListener("submit", async (event) => {
  event.preventDefault();
  clearError();
  const nick = el.nickInput.value.trim();
  const password = el.passwordInput.value;
  if (!nick || !password) {
    showError(STRINGS.emptyFieldPrompt);
    return;
  }
  setFormBusy(true, STRINGS.loadingLogin, el.loginBtn);
  try {
    const session = await invoke("login", { nick, password });
    showLoggedIn(session.nick);
  } catch (err) {
    showError(mapErrorCode(String(err)));
  } finally {
    setFormBusy(false, "", el.loginBtn);
  }
});

el.registerBtn.addEventListener("click", async () => {
  clearError();
  const nick = el.nickInput.value.trim();
  const password = el.passwordInput.value;
  if (!nick || !password) {
    showError(STRINGS.emptyFieldPrompt);
    return;
  }
  setFormBusy(true, STRINGS.loadingRegister, el.registerBtn);
  try {
    await invoke("register", { nick, password });
    // Same two fields already hold the credentials just registered —
    // log straight in rather than making the person retype anything.
    const session = await invoke("login", { nick, password });
    showLoggedIn(session.nick);
  } catch (err) {
    showError(mapErrorCode(String(err)));
  } finally {
    setFormBusy(false, "", el.registerBtn);
  }
});

el.logoutBtn.addEventListener("click", async () => {
  const nick = el.sessionNick.textContent;
  await invoke("logout", { nick });
  el.passwordInput.value = "";
  showForm(nick);
});

el.openLogBtn.addEventListener("click", async () => {
  try {
    await invoke("open_log");
  } catch {
    // Best-effort: the button's whole job is convenience, never a
    // precondition for anything else in the window.
  }
});

// --- Play: the whole sequence over a Tauri channel (D-07/LNCH-05) -------

let lastStep = { name: "", current: 0, total: 0 };
let lastRate = null;

function renderStepLabel() {
  let text = lastStep.total > 0 ? `${lastStep.name} ${lastStep.current}/${lastStep.total}` : lastStep.name;
  if (lastRate != null) {
    text += ` · ${lastRate}`;
  }
  el.stepLabel.textContent = text;
  el.progressBar.value = lastStep.total > 0 ? Math.round((lastStep.current / lastStep.total) * 100) : 0;
}

function formatRate(bytesPerSec) {
  if (bytesPerSec >= 1024 * 1024) {
    return `${(bytesPerSec / (1024 * 1024)).toFixed(1)} MB/s`;
  }
  if (bytesPerSec >= 1024) {
    return `${(bytesPerSec / 1024).toFixed(0)} KB/s`;
  }
  return `${bytesPerSec} B/s`;
}

// Fed by the real work, not a guess: every `Step`/`Bytes` event this
// handler receives came straight from the core's own sync/Java/Mojang/
// Forge progress reporting, over the channel, never the event bus.
function handleProgress(msg) {
  switch (msg.event) {
    case "Step":
      lastStep = msg.data;
      lastRate = null;
      renderStepLabel();
      break;
    case "Bytes":
      lastRate = formatRate(msg.data.per_sec);
      renderStepLabel();
      break;
    case "Done":
      // The invoke() promise's own resolution (below) owns button/label
      // restoration; nothing further to render here.
      break;
    case "Failed":
      // D-08: a failed step stops the bar where it is rather than
      // resetting or continuing — the invoke() promise's rejection (which
      // carries the full { code, reopen_form } detail, unlike this event)
      // owns the error banner and button restoration.
      break;
    default:
      break;
  }
}

function setPlayBusy(busy) {
  el.playBtn.disabled = busy;
  el.gameFolderBtn.disabled = busy;
  el.verifyFilesBtn.disabled = busy;
  el.playBtn.textContent = busy ? STRINGS.loadingLaunching : STRINGS.ctaPlay;
}

el.playBtn.addEventListener("click", async () => {
  clearError();
  setPlayBusy(true);
  lastStep = { name: "", current: 0, total: 0 };
  lastRate = null;
  el.progressArea.hidden = false;
  el.progressBar.value = 0;
  el.stepLabel.textContent = "";

  const channel = new Channel();
  channel.onmessage = handleProgress;

  try {
    await invoke("play", { onEvent: channel, nick: currentNick, ram: parseFloat(el.ramSlider.value) });
    // Success: the game is spawned and the launcher window stays open
    // behind it (D-18) — nothing further to render.
  } catch (err) {
    if (err && typeof err === "object" && "code" in err) {
      showError(mapPlayErrorCode(err.code));
      if (err.reopen_form) {
        showForm(currentNick);
      }
    } else {
      showError(STRINGS.errorGeneric);
    }
  } finally {
    setPlayBusy(false);
  }
});

// --- Verify files / Game folder ------------------------------------------

function setVerifyBusy(busy) {
  el.verifyFilesBtn.disabled = busy;
  el.playBtn.disabled = busy;
  el.gameFolderBtn.disabled = busy;
  el.verifyFilesBtn.textContent = busy ? STRINGS.loadingVerifying : STRINGS.btnVerifyFiles;
}

el.verifyFilesBtn.addEventListener("click", async () => {
  clearError();
  setVerifyBusy(true);
  lastStep = { name: "", current: 0, total: 0 };
  lastRate = null;
  el.progressArea.hidden = false;
  el.progressBar.value = 0;
  el.stepLabel.textContent = "";

  const channel = new Channel();
  channel.onmessage = handleProgress;

  try {
    const report = await invoke("verify_files", { onEvent: channel });
    if (report.repaired > 0) {
      showInfo(STRINGS.infoFilesRepaired);
    }
  } catch {
    showError(STRINGS.errorGeneric);
  } finally {
    setVerifyBusy(false);
    el.progressArea.hidden = true;
  }
});

el.gameFolderBtn.addEventListener("click", async () => {
  try {
    await invoke("open_game_folder");
  } catch {
    // Best-effort, same as Open log.
  }
});

(async () => {
  STRINGS = await invoke("get_strings");
  applyStaticCopy();

  const version = await invoke("get_version");
  const pack = await invoke("pack_version");
  el.versionFooter.textContent = `Launcher ${version} · Pack ${pack ?? "—"}`;

  systemMemory = await invoke("system_memory");
  el.ramSlider.value = systemMemory.recommended_gb;

  await tryRestoreSession();
  await pollStatus();
  setInterval(pollStatus, 15000);
})();
