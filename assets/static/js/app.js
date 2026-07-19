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
});
