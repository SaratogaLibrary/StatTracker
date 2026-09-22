(() => {
  const form = document.getElementById("settings-form");
  const deskSelect = document.getElementById("desk_id");
  const templateSelect = document.getElementById("active_template");
  const deskStatus = document.getElementById("desk-status");
  const formStatus = document.getElementById("form-status");
  const saveButton = document.getElementById("save");

  let desks = [];
  let invoke = null;

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
    document.getElementById("update_mode").value = config.update_mode === "silent" ? "silent" : "notify";
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
    await api.event.listen("config-saved", (event) => {
      const bootstrap = event.payload || {};
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
            updateMode: document.getElementById("update_mode").value,
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
