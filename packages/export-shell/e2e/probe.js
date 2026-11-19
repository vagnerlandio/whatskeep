// Tenta buscar recursos remotos de dentro da página (deve ser bloqueado pela CSP).
new Image().src = "https://example.com/e2e-probe.png?" + Date.now();
const s = document.createElement("script");
s.src = "https://example.com/e2e-probe.js";
document.head.appendChild(s);
fetch("https://example.com/e2e-probe").catch(() => {});
document.documentElement.dataset.probe = "ran";
