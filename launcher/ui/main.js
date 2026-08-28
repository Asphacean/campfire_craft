// Task 1 stub: proves the bridge works — window.__TAURI__.core.invoke
// reaches Rust, with no bundler and no ES module syntax used in this file.
// Task 3 (this plan) replaces this with the real form/session/status logic.
window.__TAURI__.core.invoke("get_version").then((version) => {
  document.getElementById("version-footer").textContent = `Launcher ${version}`;
});
