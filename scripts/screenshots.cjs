// Takes the README screenshots (docs/screenshots/*.png) from the real desktop UI.
// Starts the Vite dev server, loads it in Chromium with a mocked Tauri `invoke`
// that returns synthetic English data (fictional numbers, no real backup), and
// captures each screen in light and dark mode.
// Usage: pnpm screenshots
const { spawn } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");

const ROOT = path.resolve(__dirname, "..");
const pw = require(require.resolve("playwright-core", { paths: [path.join(ROOT, "packages/export-shell")] }));
const OUT = path.join(ROOT, "docs/screenshots");
const URL = "http://localhost:1420";

// ---------------------------------------------------------------- synthetic data

const DAY = 86_400_000;
const today = new Date();
today.setHours(0, 0, 0, 0);
const at = (daysAgo, hh, mm) => today.getTime() - daysAgo * DAY + (hh * 60 + mm) * 60_000;

// Fictional numbers only (NANP 555-01xx, Ofcom 07700 900xxx, ACMA 0491 570 xxx).
const person = (phone) => ({ jid: `${phone}@s.whatsapp.net`, phone, name: null });
const ANA = person("12025550142");
const BEN = person("447700900123");
const CHLOE = person("61491570156");
const DAN = person("12025550187");
const EVA = person("447700900456");

function landscape(w, h, [sky1, sky2, sun, m1, m2, m3]) {
  const y = (f) => Math.round(h * f);
  const x = (f) => Math.round(w * f);
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${w} ${h}">
<defs><linearGradient id="s" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="${sky1}"/><stop offset="1" stop-color="${sky2}"/></linearGradient></defs>
<rect width="${w}" height="${h}" fill="url(#s)"/>
<circle cx="${x(0.72)}" cy="${y(0.26)}" r="${Math.round(Math.min(w, h) * 0.09)}" fill="${sun}" opacity="0.9"/>
<path d="M0 ${y(0.62)} L${x(0.16)} ${y(0.38)} L${x(0.3)} ${y(0.52)} L${x(0.5)} ${y(0.28)} L${x(0.7)} ${y(0.55)} L${x(0.85)} ${y(0.42)} L${w} ${y(0.58)} V${h} H0 Z" fill="${m1}"/>
<path d="M0 ${y(0.74)} Q${x(0.25)} ${y(0.6)} ${x(0.5)} ${y(0.72)} T${w} ${y(0.68)} V${h} H0 Z" fill="${m2}"/>
<path d="M0 ${y(0.88)} Q${x(0.3)} ${y(0.78)} ${x(0.6)} ${y(0.86)} T${w} ${y(0.84)} V${h} H0 Z" fill="${m3}"/>
</svg>`;
  return "data:image/svg+xml;charset=utf-8," + encodeURIComponent(svg);
}

const images = {
  105: landscape(1600, 1200, ["#8ec5fc", "#e0f0ff", "#fff3b0", "#5b7c99", "#3f6e4f", "#2c4f37"]),
  107: landscape(1200, 1600, ["#ffb37b", "#ffe1c4", "#fff6d5", "#7a6a8f", "#4d6b5a", "#30463a"]),
};

// Ids start at 100 in the order below: the photos are 105 and 107 (see `images`).
let nextId = 100;
function msg(chatId, sentAt, from, kind, text, extra = {}) {
  return {
    id: nextId++,
    chatId,
    sentAt,
    fromMe: from === "me",
    kind,
    text,
    sender: from === "me" ? null : from,
    starred: false,
    edited: false,
    systemAction: null,
    quoted: null,
    media: null,
    location: null,
    contacts: [],
    reactions: [],
    ...extra,
  };
}
const media = (o) => ({
  mime: null, size: null, width: null, height: null, durationS: null, fileName: null,
  transcription: null, hasThumbnail: false, available: true, ...o,
});

const HIKE = 1;
const hike = [
  msg(HIKE, at(4, 19, 2), ANA, "text", "Who's in for Saturday? Thinking Eagle Ridge trail, about 12 km 🥾"),
  msg(HIKE, at(4, 19, 5), BEN, "text", "I'm in! What time do we start?"),
  msg(HIKE, at(4, 19, 7), "me", "text", "Count me in. 7:30 at the trailhead parking?"),
  msg(HIKE, at(4, 19, 10), CHLOE, "text", "Works for me ☀️", { reactions: [{ emoji: "👍", count: 3, mine: true }] }),
  msg(HIKE, at(4, 19, 12), ANA, "document", null, {
    media: media({ mime: "application/pdf", size: 2_516_582, fileName: "eagle-ridge-trail-map.pdf" }),
  }),
  msg(HIKE, at(2, 7, 24), ANA, "image", "Made it to the first viewpoint!", {
    media: media({ mime: "image/jpeg", width: 1600, height: 1200, hasThumbnail: true }),
  }),
  msg(HIKE, at(2, 7, 26), BEN, "text", "Wow, that view 😍", {
    quoted: { messageId: 105, text: "Made it to the first viewpoint!", fromMe: false, sender: ANA },
  }),
  msg(HIKE, at(2, 9, 41), "me", "image", "Summit! 1,284 m", {
    media: media({ mime: "image/jpeg", width: 1200, height: 1600, hasThumbnail: true }),
    reactions: [{ emoji: "❤️", count: 3, mine: false }, { emoji: "🔥", count: 1, mine: false }],
  }),
  msg(HIKE, at(2, 9, 45), CHLOE, "voice", null, {
    media: media({ mime: "audio/ogg", durationS: 42, transcription: "The descent on the north side is muddy, take the left path at the fork." }),
  }),
  msg(HIKE, at(2, 12, 10), ANA, "location", null, {
    location: { latitude: 39.12345, longitude: -105.54321, name: "Eagle Ridge Trailhead", address: "Parking lot B" },
  }),
  msg(HIKE, at(1, 20, 15), BEN, "text", "Same time next month? 📅"),
  msg(HIKE, at(1, 20, 18), "me", "text", "Absolutely. And I'm keeping this chat forever, the photos are too good to lose", { starred: true }),
  msg(HIKE, at(1, 20, 22), ANA, "text", "Sharing the full album here tonight 📸"),
];

const last = (m) => ({ kind: m.kind, text: m.text, fromMe: m.fromMe, sender: m.sender });
function chat(id, kind, title, contact, lastAt, lastMsg, extra = {}) {
  return {
    id, kind, title, jid: contact.jid, phone: kind === "direct" ? contact.phone : null, name: null,
    archived: false, messageCount: 50 + id * 37, lastMessageAt: lastAt, last: lastMsg, ...extra,
  };
}
const group = (n) => ({ jid: `1203630000000${n}@g.us`, phone: null });
const L = (kind, text, from) => ({ kind, text, fromMe: from === "me", sender: from === "me" ? null : from });

const chats = [
  chat(HIKE, "group", "Weekend Hiking Crew", group(1), hike.at(-1).sentAt, last(hike.at(-1))),
  chat(2, "group", "Family", group(2), at(1, 18, 4), L("image", null, DAN)),
  chat(3, "direct", null, DAN, at(1, 17, 31), L("text", "See you at dinner tomorrow!", "me")),
  chat(4, "group", "Book Club", group(4), at(2, 21, 50), L("text", "Chapter 12 was wild 📚", EVA)),
  chat(5, "direct", null, BEN, at(3, 8, 12), L("voice", null, BEN)),
  chat(6, "group", "Design Team", group(6), at(3, 16, 40), L("document", null, "me")),
  chat(7, "direct", null, CHLOE, at(5, 11, 3), L("text", "Thanks for the recommendation!", CHLOE)),
  chat(8, "group", "Neighbors", group(8), at(6, 9, 15), L("text", "Package left at the front desk", ANA)),
  chat(9, "direct", null, EVA, at(8, 14, 22), L("deleted", null, EVA)),
  chat(10, "direct", null, ANA, at(12, 10, 0), L("location", null, ANA)),
  chat(11, "group", "Old Apartment", group(11), at(200, 10, 0), L("text", "Keys returned, all done!", "me"), { archived: true }),
  chat(12, "direct", null, person("12025550111"), at(320, 10, 0), L("text", "Got it, thanks", "me"), { archived: true }),
];

const DATA = {
  stores: [
    { sourceApp: "whatsapp", importedAtMs: at(0, 9, 30), sourceSchema: "modern", chats: 128, messages: 48_213 },
    { sourceApp: "business", importedAtMs: at(0, 9, 42), sourceSchema: "modern", chats: 42, messages: 9_310 },
  ],
  chats,
  messages: { [HIKE]: hike },
  images,
};

// ---------------------------------------------------------------- mocked Tauri (runs in the page)

function installMock({ state, data }) {
  const byChat = data.messages;
  const all = Object.values(byChat).flat();
  const key = (m) => [m.sentAt, m.id];
  const lt = (a, b) => a[0] < b[0] || (a[0] === b[0] && a[1] < b[1]);
  let cb = 0;
  const handlers = {
    vault_status: () => ({ state, autolockSecs: 300 }),
    get_language: () => "en",
    list_stores: () => data.stores,
    list_chats: () => data.chats,
    get_messages: ({ chatId, direction, cursor, limit }) => {
      const list = byChat[chatId] ?? [];
      if (direction === "before") {
        const before = cursor ? list.filter((m) => lt(key(m), [cursor.sentAt, cursor.id])) : list;
        return before.slice(-limit);
      }
      return list.filter((m) => lt([cursor.sentAt, cursor.id], key(m))).slice(0, limit);
    },
    get_messages_around: ({ chatId, anchor }) => {
      const list = byChat[chatId] ?? [];
      const i = anchor.type === "message" ? list.findIndex((m) => m.id === anchor.id) : list.findIndex((m) => m.sentAt >= anchor.at);
      return { messages: list, anchorIndex: i < 0 ? null : i };
    },
    search_messages: ({ query, chatId, before }) => {
      const q = query.trim().toLowerCase();
      return all
        .filter((m) => m.text && m.text.toLowerCase().includes(q) && (chatId == null || m.chatId === chatId))
        .filter((m) => !before || lt(key(m), [before.sentAt, before.id]))
        .sort((a, b) => b.sentAt - a.sentAt)
        .map((m) => {
          const i = m.text.toLowerCase().indexOf(q);
          const snippet = m.text.slice(0, i) + "\u0001" + m.text.slice(i, i + q.length) + "\u0002" + m.text.slice(i + q.length);
          return { messageId: m.id, chatId: m.chatId, sentAt: m.sentAt, fromMe: m.fromMe, sender: m.sender, snippet };
        });
    },
    "plugin:event|listen": () => ++cb,
  };
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: "main" }, currentWebview: { windowLabel: "main", label: "main" } },
    transformCallback: () => ++cb,
    convertFileSrc: (p) => data.images[p.split("/").pop()] ?? "",
    invoke: async (cmd, args) => (handlers[cmd] ? handlers[cmd](args ?? {}) : null),
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
}

// ---------------------------------------------------------------- capture

const serverUp = () => fetch(URL).then((r) => r.ok, () => false);

async function waitForServer() {
  for (let i = 0; i < 120; i++) {
    if (await serverUp()) return;
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error("Vite dev server did not start on " + URL);
}

async function open(browser, state, scheme) {
  const page = await browser.newPage({
    viewport: { width: 1200, height: 800 },
    deviceScaleFactor: 2,
    locale: "en-US",
    colorScheme: scheme,
  });
  page.on("pageerror", (e) => console.error("page error:", e.message));
  await page.addInitScript(installMock, { state, data: DATA });
  await page.goto(URL);
  return page;
}

const shot = async (page, name) => {
  await page.waitForTimeout(600);
  await page.screenshot({ path: path.join(OUT, `${name}.png`) });
  console.log("wrote", path.relative(ROOT, path.join(OUT, `${name}.png`)));
};

async function openHikeChat(page) {
  await page.waitForSelector(".chat-item");
  await page.click(".chat-item >> nth=0");
  await page.waitForSelector("text=Sharing the full album");
  // Scroll up a little so both photos are in view.
  await page.waitForTimeout(800);
  await page.locator(".messages").evaluate((el) => el.scrollBy(0, -300));
}

(async () => {
  fs.mkdirSync(OUT, { recursive: true });
  // Reuses a dev server that is already running (e.g. `pnpm dev`); otherwise starts one.
  const vite = (await serverUp())
    ? null
    : spawn("pnpm", ["--filter", "desktop", "exec", "vite", "--port", "1420", "--strictPort"], {
        cwd: ROOT,
        stdio: "ignore",
        detached: true,
      });
  const browser = await pw.chromium.launch();
  try {
    await waitForServer();

    for (const scheme of ["light", "dark"]) {
      const page = await open(browser, "unlocked", scheme);
      await openHikeChat(page);
      await shot(page, `chat-${scheme}`);
      await page.close();
    }

    {
      const page = await open(browser, "unlocked", "light");
      await openHikeChat(page);
      await page.fill(".search-box input", "trail");
      await page.waitForSelector(".search-hit");
      await shot(page, "search");
      await page.close();
    }

    {
      const page = await open(browser, "locked", "light");
      await page.fill("input[type=password]", "a long passphrase");
      await shot(page, "unlock");
      await page.close();
    }

    {
      const page = await open(browser, "unlocked", "light");
      await page.waitForSelector(".chat-item");
      await page.click(".rail-btn[title='Import or export']");
      await page.waitForSelector(".home");
      await shot(page, "import");
      await page.close();
    }
  } finally {
    await browser.close();
    if (vite) process.kill(-vite.pid);
  }
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
