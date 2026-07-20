document.addEventListener("htmx:configRequest", function (event) {
  const method = String(event.detail.verb || "GET").toUpperCase();
  if (!["POST", "PUT", "PATCH", "DELETE"].includes(method)) return;
  const target = new URL(event.detail.path, window.location.origin);
  if (target.origin !== window.location.origin) return;
  const token = document.querySelector('meta[name="csrf-token"]')?.content;
  if (token) event.detail.headers["X-CSRF-Token"] = token;
});

document.addEventListener("htmx:beforeSwap", function (event) {
  const loginResponse = event.detail.target?.id === "login-region";
  const swappableStatus = [401, 422, 429, 503].includes(event.detail.xhr.status);
  if (loginResponse && swappableStatus) {
    event.detail.shouldSwap = true;
    event.detail.isError = false;
  }

  const customerResponse = [
    "customer-form",
    "customer-list-content",
    "customer-detail",
    "vehicle-form",
    "attachment-form",
    "main-content",
  ]
    .includes(event.detail.target?.id);
  if (customerResponse && [409, 422].includes(event.detail.xhr.status)) {
    event.detail.shouldSwap = true;
    event.detail.isError = false;
  }
});

document.addEventListener("click", function (event) {
  const openButton = event.target.closest("[data-dialog-open]");
  if (openButton) {
    const dialog = document.getElementById(openButton.dataset.dialogOpen);
    if (dialog?.showModal) dialog.showModal();
    return;
  }
  const closeButton = event.target.closest("[data-dialog-close]");
  if (closeButton) closeButton.closest("dialog")?.close();
});

document.addEventListener("keydown", function (event) {
  if (event.key !== "Escape") return;
  document.querySelectorAll(".more-menu[open]").forEach(function (menu) {
    menu.removeAttribute("open");
    menu.querySelector("summary")?.focus();
  });
});
