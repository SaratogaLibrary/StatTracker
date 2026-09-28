(() => {
  const form = document.getElementById("settings-form");
  const deskSelect = document.getElementById("desk_id");
  const templateSelect = document.getElementById("active_template");
  const deskStatus = document.getElementById("desk-status");
  const formStatus = document.getElementById("form-status");
  const saveButton = document.getElementById("save");
  const updateModeSelect = document.getElementById("update_mode");
  const updateModeHint = document.getElementById("update-mode-hint");
  const checkUpdatesButton = document.getElementById("check-updates");

  let desks = [];
  let invoke = null;
  let dialog = null;

  async function whenTauriReady() {
    for (let attempt = 0; attempt < 80; attempt += 1) {
      const api = window.__TAURI__;
      if (api?.tauri?.invoke) {
        return api;
      }
      await new Promise((resolve) => window.setTimeout(resolve, 50));
    }
    throw new Error("The settings window failed to connect to the application.");
  }

  const fill = (id, value) => {
    const node = document.getElementById(id);
    if (node) {
      node.value = value ?? "";
    }
  };

  function normalizeUpdateMode(mode) {
    if (mode === "silent" || mode === "manual") {
      return mode;
    }
    return "notify";
  }

  function syncUpdateModeUi() {
    const mode = normalizeUpdateMode(updateModeSelect.value);
    updateModeSelect.value = mode;
    checkUpdatesButton.hidden = mode !== "manual";
    if (mode === "silent") {
      updateModeHint.textContent =
        "Updates download and install on startup without asking.";
    } else if (mode === "manual") {
      updateModeHint.textContent =
        "StatTracker will not check on its own. Use the button to look for a newer GitHub release.";
    } else {
      updateModeHint.textContent =
        "Notify shows GitHub release notes for versions newer than this computer.";
    }
  }

  async function promptUpdate(notice) {
    const version = notice?.version || notice;
    const current = String(notice?.current_version || notice?.currentVersion || "").replace(
      /^v/i,
      "",
    );
    const notes = String(notice?.notes || "").trim();
    let message = `StatTracker ${version} is available`;
    if (current && current !== String(version).replace(/^v/i, "")) {
      message += ` (you have ${current})`;
    }
    message += ". Install it now?";
    if (notes) {
      message += `\n\n${notes}`;
    }
    const accepted = await dialog.ask(message, {
      title: "Update available",
      type: "info",
    });
    if (accepted) {
      await invoke("install_update");
    }
  }

  async function checkForUpdates() {
    checkUpdatesButton.disabled = true;
    try {
      const notice = await invoke("check_updates", { interactive: true });
      if (notice) {
        await promptUpdate(notice);
      } else {
        await dialog.message("StatTracker is up to date.", {
          title: "Updates",
          type: "info",
        });
      }
    } catch (error) {
      await dialog.message(String(error), {
        title: "Updates",
        type: "error",
      });
    } finally {
      checkUpdatesButton.disabled = false;
    }
  }

  function populateDesks(list, selectedId) {
    desks = list;
    deskSelect.replaceChildren();
    const placeholder = document.createElement("option");
    placeholder.value = "";
    placeholder.textContent = list.length ? "Choose a desk" : "No desks available";
    deskSelect.appendChild(placeholder);
    list.forEach((desk) => {
      const option = document.createElement("option");
      option.value = String(desk.id);
      option.textContent = desk.name;
      if (Number(selectedId) === desk.id) {
        option.selected = true;
      }
      deskSelect.appendChild(option);
    });
  }

  function populateTemplates(list, selected) {
    if (!list.length) {
      return;
    }
    templateSelect.replaceChildren();
    list.forEach((template) => {
      const option = document.createElement("option");
      option.value = template.id;
      option.textContent = template.name;
      if (template.id === selected) {
        option.selected = true;
      }
      templateSelect.appendChild(option);
    });
  }

  async function loadDesks(selectedId) {
    const baseUrl = document.getElementById("base_url").value.trim();
    if (!baseUrl) {
      deskStatus.textContent = "Enter a base URL to load desks.";
      return;
    }
    deskStatus.textContent = "Loading desks from the server…";
    try {
      const list = await invoke("fetch_desks", {
        baseUrl,
      });
      populateDesks(list, selectedId);
      deskStatus.textContent = list.length
        ? "Desk list updated from the server."
        : "The server returned no desks.";
    } catch (error) {
      populateDesks(desks, selectedId);
      deskStatus.textContent = String(error);
    }
  }

  async function hydrate() {
    const bootstrap = await invoke("get_bootstrap");
    window.StatTrackerSystemWindow?.applyFromBootstrap(bootstrap);
    const config = bootstrap.config;
    fill("base_url", config.base_url);
    fill("org_name", config.org_name);
    fill("window_title", config.window_title || "StatTracker");
    fill("storage_directory", bootstrap.pending_storage || config.storage_directory || bootstrap.default_storage);
    fill("unfocused_opacity", String(config.unfocused_opacity ?? 55));
    fill("inactivity_minutes", String(config.inactivity_minutes || 15));
    fill("inactivity_message", config.inactivity_message);
    document.getElementById("autostart").checked = config.autostart !== false;
    document.getElementById("always_on_top").checked = Boolean(config.always_on_top);
    document.getElementById("inactivity_enabled").checked = Boolean(config.inactivity_enabled);
    updateModeSelect.value = normalizeUpdateMode(config.update_mode);
    syncUpdateModeUi();
    populateTemplates(bootstrap.templates, config.active_template || "vertical");
    if (bootstrap.last_error && !bootstrap.question_types.length) {
      formStatus.textContent = bootstrap.last_error;
    }
    const cached = bootstrap.desks || [];
    populateDesks(cached, config.desk_id);
    if (cached.length) {
      deskStatus.textContent = "";
    } else if (config.base_url) {
      await loadDesks(config.desk_id);
    }
  }

  async function start() {
    const api = await whenTauriReady();
    invoke = api.tauri.invoke;
    dialog = api.dialog;
    window.StatTrackerSystemWindow?.bindEscapeToClose(invoke);
    updateModeSelect.addEventListener("change", syncUpdateModeUi);
    checkUpdatesButton.addEventListener("click", () => {
      checkForUpdates().catch((error) => {
        formStatus.textContent = String(error);
      });
    });
    await api.event.listen("config-saved", (event) => {
      const bootstrap = event.payload || {};
      window.StatTrackerSystemWindow?.applyFromBootstrap(bootstrap);
      if (bootstrap.config?.active_template) {
        templateSelect.value = bootstrap.config.active_template;
      }
      if (bootstrap.question_types && bootstrap.question_types.length) {
        return;
      }
      formStatus.textContent =
        bootstrap.last_error ||
        "Could not load question types. Check the base URL and try saving again.";
      saveButton.disabled = false;
    });

    document.getElementById("browse").addEventListener("click", async () => {
      try {
        const path = await invoke("pick_directory");
        if (path) {
          fill("storage_directory", path);
        }
      } catch (error) {
        formStatus.textContent = String(error);
      }
    });

    document.getElementById("unfocused_opacity").addEventListener("input", () => {
      api.event.emit("appearance-preview", {
        unfocused_opacity: Number(document.getElementById("unfocused_opacity").value),
      });
    });
    document.getElementById("refresh-desks").addEventListener("click", () => {
      loadDesks(deskSelect.value);
    });
    document.getElementById("base_url").addEventListener("change", () => {
      if (!desks.length) {
        loadDesks(deskSelect.value);
      }
    });
    document.getElementById("base_url").addEventListener("blur", () => {
      const input = document.getElementById("base_url");
      const value = input.value.trim();
      if (value && !value.endsWith("/")) {
        input.value = `${value}/`;
      }
    });

    form.addEventListener("submit", async (event) => {
      event.preventDefault();
      formStatus.textContent = "";
      const deskId = Number(deskSelect.value);
      const desk = desks.find((item) => item.id === deskId);
      if (!deskId || !desk) {
        formStatus.textContent = "Select a desk before saving.";
        return;
      }
      saveButton.disabled = true;
      try {
        const bootstrap = await invoke("save_config", {
          input: {
            baseUrl: document.getElementById("base_url").value.trim(),
            deskId,
            deskName: desk.name,
            orgName: document.getElementById("org_name").value.trim(),
            windowTitle: document.getElementById("window_title").value.trim(),
            updateMode: normalizeUpdateMode(updateModeSelect.value),
            autostart: document.getElementById("autostart").checked,
            alwaysOnTop: document.getElementById("always_on_top").checked,
            unfocusedOpacity: Number(document.getElementById("unfocused_opacity").value),
            inactivityEnabled: document.getElementById("inactivity_enabled").checked,
            inactivityMinutes: Number(document.getElementById("inactivity_minutes").value),
            inactivityMessage: document.getElementById("inactivity_message").value,
            activeTemplate: document.getElementById("active_template").value,
          },
        });
        formStatus.textContent = bootstrap?.question_types?.length
          ? "Saved."
          : "Saved. Loading question types from the server…";
      } catch (error) {
        formStatus.textContent = String(error);
        saveButton.disabled = false;
      }
    });

    await hydrate();
  }

  window.addEventListener("DOMContentLoaded", () => {
    start().catch((error) => {
      formStatus.textContent = String(error);
    });
  });
})();
