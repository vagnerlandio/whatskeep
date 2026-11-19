// Abre pacotes gerados pelo wa-export via file:// no Chromium e no WebKit e
// confere senha, decifragem, busca, mídias, isolamento por senha e CSP (sem rede).
// Uso: pnpm --filter export-shell test:e2e
const pw = require("playwright-core");
const OUT = process.argv[2];
let failed = 0;
const results = [];
const check = (name, ok, extra = "") => (ok || failed++, results.push(`${ok ? "OK  " : "FALHOU"} ${name}${extra ? " — " + extra : ""}`));

async function openPkg(browser, pkg) {
  const page = await browser.newPage({ viewport: { width: 1200, height: 800 } });
  const problems = [];
  page.on("request", (r) => { if (!r.url().startsWith("file:") && !r.url().startsWith("blob:") && !r.url().startsWith("data:")) problems.push("REDE " + r.url()); });
  page.on("console", (m) => { if (m.type() === "error" || /Content Security Policy|Refused/i.test(m.text())) problems.push("CONSOLE " + m.text()); });
  page.on("pageerror", (e) => problems.push("ERRO " + e.message));
  await page.goto(`file://${OUT}/${pkg}/index.html`);
  return { page, problems };
}

async function unlock(page, password) {
  await page.fill("input[type=password]", password);
  await page.click("button.primary");
}

(async () => {
  for (const [bname, btype] of [["chromium", pw.chromium], ["webkit", pw.webkit]]) {
    const browser = await btype.launch();

    // 1) Mídias cifradas
    {
      const { page, problems } = await openPkg(browser, "cifrado");
      await page.waitForSelector("input[type=password]");
      await unlock(page, "senha errada qualquer");
      const wrong = await page.waitForSelector("text=Senha incorreta", { timeout: 30000 }).then(() => true, () => false);
      check(`${bname}: senha errada recusada`, wrong);
      const t0 = Date.now();
      await unlock(page, "senha do export");
      await page.waitForSelector(".chat-item", { timeout: 30000 });
      const argonMs = Date.now() - t0;
      const chats = await page.locator(".chat-item").count();
      const archived = await page.locator(".archived-row").count();
      check(`${bname}: conversas liberadas`, chats + archived >= 2, `${chats} na lista + arquivadas, Argon2id ~${argonMs} ms`);
      await page.click(".chat-item >> nth=0");
      const text = await page.waitForSelector("text=Vamos tomar um café", { timeout: 15000 }).then(() => true, () => false);
      check(`${bname}: mensagem decifrada aparece`, text);
      await page.fill(".search-box input", "CAFE");
      const hit = await page.waitForSelector(".search-hit mark", { timeout: 10000 }).then(async (el) => await el.textContent(), () => null);
      check(`${bname}: busca sem acento destaca o termo`, hit === "café", `marcado: ${hit}`);
      await page.fill(".search-box input", "");
      const banner = await page.locator(".folder-banner").count();
      check(`${bname}: pede a pasta para as mídias`, banner === 1);
      let folderOk = false;
      try {
        await page.setInputFiles("input[type=file]", `${OUT}/cifrado`);
        folderOk = true;
      } catch (e) {
        check(`${bname}: seleção de pasta via Playwright`, false, String(e.message).split("\n")[0]);
      }
      if (folderOk) {
        await page.click(".chat-item >> nth=0");
        const full = await page.waitForSelector(".visual img.full.loaded", { timeout: 15000 }).then(() => true, () => false);
        check(`${bname}: foto decifrada no navegador`, full);
        await page.screenshot({ path: `${OUT}/${bname}-cifrado.png` });
        await page.click(".visual.clickable");
        const lb = await page.waitForSelector(".lightbox img", { timeout: 10000 }).then(() => true, () => false);
        check(`${bname}: tela cheia abre a foto`, lb);
        await page.keyboard.press("Escape");
      }
      check(`${bname}: sem rede, sem violação de CSP, sem erros`, problems.length === 0, problems.slice(0, 3).join(" | "));
      await page.close();
    }

    // 2) Mídias em claro
    {
      const { page, problems } = await openPkg(browser, "claro");
      await page.waitForSelector("input[type=password]");
      await unlock(page, "senha do export");
      await page.waitForSelector(".chat-item", { timeout: 30000 });
      await page.click(".chat-item >> nth=0");
      const full = await page.waitForSelector(".visual img.full.loaded", { timeout: 15000 }).then(() => true, () => false);
      check(`${bname}: foto em claro sem escolher pasta`, full);
      check(`${bname}: (claro) sem rede/CSP/erros`, problems.length === 0, problems.slice(0, 3).join(" | "));
      await page.close();
    }

    // 3) Uma senha por conversa
    {
      const { page, problems } = await openPkg(browser, "por-conversa");
      await page.waitForSelector("input[type=password]");
      await unlock(page, "senha da alice");
      await page.waitForSelector(".chat-item", { timeout: 30000 });
      const a = (await page.locator(".chat-item").count()) + (await page.locator(".archived-row").count());
      await page.fill(".search-box input", "grupo");
      await page.waitForTimeout(800);
      const leaked = await page.locator(".search-hit").count();
      await page.fill(".search-box input", "");
      check(`${bname}: senha A libera só a conversa A`, a === 1, `itens=${a}`);
      check(`${bname}: busca com A não acha texto de B`, leaked === 0);
      await page.click("button[title^='Liberar mais']");
      await unlock(page, "senha do grupo");
      await page.waitForSelector(".archived-row", { timeout: 30000 });
      await page.fill(".search-box input", "grupo");
      const found = await page.waitForSelector(".search-hit", { timeout: 10000 }).then(() => true, () => false);
      check(`${bname}: com a senha B, a busca cobre as duas`, found);
      check(`${bname}: (por conversa) sem rede/CSP/erros`, problems.length === 0, problems.slice(0, 3).join(" | "));
      await page.close();
    }
    await browser.close();
  }
  // Nenhum recurso remoto, mesmo pedido por script da própria página.
  for (const [bname, btype] of [["chromium", pw.chromium], ["webkit", pw.webkit]]) {
    const browser = await btype.launch();
    const page = await browser.newPage();
    const net = [];
    page.on("response", (r) => r.url().startsWith("http") && net.push(r.url()));
    await page.goto(`file://${OUT}/cifrado/index.html`);
    await page.waitForSelector("input[type=password]");
    // Como arquivo (file:), que a CSP permite; inline seria bloqueado e o teste não provaria nada.
    // O Chromium repassa a violação de CSP causada pela sonda como erro daqui.
    let blocked = "";
    await page
      .addScriptTag({ url: "file://" + require("path").join(__dirname, "probe.js") })
      .catch((e) => (blocked = /Content Security Policy/.test(e.message) ? "CSP bloqueou o script remoto" : e.message));
    await page.waitForTimeout(1500);
    const ran = await page.evaluate(() => document.documentElement.dataset.probe === "ran");
    check(`${bname}: sonda rodou na página e nenhuma resposta veio da rede`, ran && net.length === 0, `rodou=${ran} ${blocked} ${net.join(", ")}`.trim());
    await browser.close();
  }
  console.log(results.join("\n"));
  if (failed) process.exit(1);
})().catch((e) => { console.error(e); process.exit(1); });
