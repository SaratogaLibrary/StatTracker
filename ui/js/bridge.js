(() => {
  const invoke = (...args) => window.__TAURI__.tauri.invoke(...args);

  const StatTracker = {
    config: null,
    questionTypes: [],
    stats: { last_hour: 0, today: 0, week: 0, month: 0, year: 0 },
    online: false,
    lastError: null,

    invoke,
    recordTally(questionTypeId) {
      return invoke("record_tally", { questionTypeId: Number(questionTypeId) });
    },
    refresh() {
      return invoke("refresh_question_types");
    },
    cycleTemplate() {
      return invoke("cycle_template");
    },
    openSettings() {
      return invoke("open_settings");
    },
    openHelp() {
      return invoke("open_help");
    },
  };

  window.StatTracker = StatTracker;
})();
