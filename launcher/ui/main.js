// The whole single-screen frontend: form state, button handlers, status
// polling. No bundler and no ES module syntax anywhere — everything
// reaches Rust through window.__TAURI__.core.invoke (app.withGlobalTauri
// in tauri.conf.json).
// This plan brings the auth half of the UI-SPEC to life; the RAM slider,
// progress bar, and Play flow stay inert (hidden) until a later wave wires
// manifest sync / Java / Forge / launch.

const invoke = window.__TAURI__.core.invoke;

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
  versionFooter: document.getElementById("version-footer"),
};

let STRINGS = {};

function applyStaticCopy() {
  el.loginBtn.textContent = STRINGS.ctaLogin;
  el.registerBtn.textContent = STRINGS.ctaRegister;
}

function showError(message) {
  el.errorText.textContent = message;
  el.errorBanner.hidden = false;
}

function clearError() {
  el.errorBanner.hidden = true;
  el.errorText.textContent = "";
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

function showLoggedIn(nick) {
  el.form.hidden = true;
  el.sessionLine.hidden = false;
  el.sessionNick.textContent = nick;
}

function showForm(prefillNick) {
  el.sessionLine.hidden = true;
  el.form.hidden = false;
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
  const path = await invoke("get_log_path");
  // Actually revealing the file in the OS file manager is a later-wave
  // dependency (tauri-plugin-opener, added alongside "Game folder") — for
  // now this at least tells the person exactly where it is.
  window.alert(`Log file: ${path}`);
});

(async () => {
  STRINGS = await invoke("get_strings");
  applyStaticCopy();

  const version = await invoke("get_version");
  el.versionFooter.textContent = `Launcher ${version}`;

  await tryRestoreSession();
  await pollStatus();
  setInterval(pollStatus, 15000);
})();
