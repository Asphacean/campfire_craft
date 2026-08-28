//! D-01: every user-visible sentence in the whole launcher, copied verbatim
//! from `04-UI-SPEC.md`'s Copywriting Contract, as `pub const` items. This
//! is the one file a future Russian pass touches. Rust-side errors carry a
//! variant (see [`crate::auth::AuthError`]), not a sentence — `main.js`
//! maps variants to these strings via the `get_strings` Tauri command, so
//! the copy has exactly one home even though it renders in two languages
//! of code.

pub const CTA_LOGIN: &str = "Log in";
pub const CTA_REGISTER: &str = "Create account";
pub const CTA_PLAY: &str = "Play";

pub const LOGGED_IN_PREFIX: &str = "Playing as";
pub const LOGGED_IN_SUFFIX: &str = "· Log out";

pub const EMPTY_FIELD_PROMPT: &str = "Enter a nickname and password.";

pub const LOADING_LOGIN: &str = "Logging in…";
pub const LOADING_REGISTER: &str = "Creating account…";
pub const LOADING_LAUNCHING: &str = "Launching…";
pub const LOADING_VERIFYING: &str = "Verifying…";

pub const STATUS_CHECKING: &str = "Checking…";
pub const STATUS_ONLINE: &str = "Online";
pub const STATUS_OFFLINE: &str = "Offline";

pub const ERROR_WRONG_PASSWORD: &str = "Wrong nickname or password.";
pub const ERROR_SERVER_UNREACHABLE: &str = "Can't reach campfire.pub. Check your internet connection.";
pub const ERROR_JAVA_DOWNLOAD_FAILED: &str = "Couldn't set up Java.";
pub const ERROR_DISK_FULL: &str = "Not enough disk space to continue.";
pub const ERROR_SESSION_EXPIRED: &str = "Your session expired — log in again.";
pub const ERROR_OPEN_LOG: &str = "Open log";

pub const INFO_FILES_REPAIRED: &str = "Some files didn't match and were repaired.";

pub const UPDATE_DIALOG_TITLE: &str = "Update available";
pub const UPDATE_BUTTON_NOW: &str = "Update now";
pub const UPDATE_BUTTON_LATER: &str = "Later";

pub const RAM_WARNING: &str = ">70% of your system RAM — this may slow down other apps.";

pub const BTN_GAME_FOLDER: &str = "Game folder";
pub const BTN_VERIFY_FILES: &str = "Verify files";

/// Serializes every string above into the JSON blob `main.js` receives from
/// the `get_strings` Tauri command — the copy's single home, rendered once
/// into whichever language of code needs it.
pub fn as_json() -> serde_json::Value {
    serde_json::json!({
        "ctaLogin": CTA_LOGIN,
        "ctaRegister": CTA_REGISTER,
        "ctaPlay": CTA_PLAY,
        "loggedInPrefix": LOGGED_IN_PREFIX,
        "loggedInSuffix": LOGGED_IN_SUFFIX,
        "emptyFieldPrompt": EMPTY_FIELD_PROMPT,
        "loadingLogin": LOADING_LOGIN,
        "loadingRegister": LOADING_REGISTER,
        "loadingLaunching": LOADING_LAUNCHING,
        "loadingVerifying": LOADING_VERIFYING,
        "statusChecking": STATUS_CHECKING,
        "statusOnline": STATUS_ONLINE,
        "statusOffline": STATUS_OFFLINE,
        "errorWrongPassword": ERROR_WRONG_PASSWORD,
        "errorServerUnreachable": ERROR_SERVER_UNREACHABLE,
        "errorJavaDownloadFailed": ERROR_JAVA_DOWNLOAD_FAILED,
        "errorDiskFull": ERROR_DISK_FULL,
        "errorSessionExpired": ERROR_SESSION_EXPIRED,
        "errorOpenLog": ERROR_OPEN_LOG,
        "infoFilesRepaired": INFO_FILES_REPAIRED,
        "updateDialogTitle": UPDATE_DIALOG_TITLE,
        "updateButtonNow": UPDATE_BUTTON_NOW,
        "updateButtonLater": UPDATE_BUTTON_LATER,
        "ramWarning": RAM_WARNING,
        "btnGameFolder": BTN_GAME_FOLDER,
        "btnVerifyFiles": BTN_VERIFY_FILES,
    })
}
