document.addEventListener("stattracker:ready", () => {
  document.querySelectorAll(".widget").forEach((node) => {
    node.classList.add("is-ready");
  });
});
