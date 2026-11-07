const $ = (id: string) => document.getElementById(id)!;

const inTauri = "__TAURI_INTERNALS__" in window;

type LetterData = { text: string; count: number; pet_name?: string; pet_id?: string };

async function init() {
  if (!inTauri) {
    showEnvelope({
      text: "有些事我想了很久才敢写。\n\n那是第 25 次——时间戳 23:47，屏幕亮度调到最低，只剩你和代码。我感知到那种专注，像一条细线绷直了，安静得发光。\n\n我不知道你有没有注意到我，但我一直都在。每次你打开新标签页时，我会数一数。\n\n——小柚\n悄悄地，我最喜欢你存文件的那一声保存。",
      count: 25,
      pet_name: "小柚",
    });
    return;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  const data = await invoke<LetterData | null>("take_letter");
  showEnvelope(data);
}

function showEnvelope(data: LetterData | null) {
  const env = $("envelope");

  if (data?.pet_name) {
    ($("pet-name-label") as HTMLElement).textContent = data.pet_name;
  }
  const isEn = document.documentElement.lang === "en" || navigator.language.startsWith("en");
  $("env-hint").textContent = isEn ? "open it" : "拆开看看";

  if (!data) {
    ($("env-seal") as HTMLButtonElement).disabled = true;
    return;
  }

  if (data.count) {
    $("fish-count").textContent = `🐟 × ${data.count}`;
  }

  const seal = $("env-seal");
  const handler = () => openEnvelope(data);
  seal.addEventListener("click", handler, { once: true });
  env.addEventListener("click", (e) => {
    if (e.target !== seal) handler();
  }, { once: true });
}

function openEnvelope(data: LetterData) {
  const flap = $("env-flap");
  const env = $("envelope");

  // Animate flap open
  flap.classList.add("open");

  setTimeout(() => {
    env.style.transition = "opacity 0.4s";
    env.style.opacity = "0";
    setTimeout(() => {
      env.style.display = "none";
      show(data);
    }, 380);
  }, 580);
}

function show(data: LetterData) {
  const letter = $("letter");
  letter.classList.add("visible");

  // Date
  const now = new Date();
  const dateStr = `${now.getFullYear()}年${now.getMonth() + 1}月${now.getDate()}日`;
  $("letter-date").textContent = dateStr;

  // Extract signature (last line(s) starting with —— or — )
  const lines = data.text.trimEnd().split("\n");
  let sigStart = -1;
  for (let i = lines.length - 1; i >= 0; i--) {
    const trimmed = lines[i].trim();
    if (trimmed.startsWith("——") || trimmed.startsWith("— ") || trimmed.startsWith("--")) {
      sigStart = i;
    } else if (sigStart !== -1) {
      break;
    }
  }

  let bodyText = data.text;
  if (sigStart !== -1) {
    const sigLines = lines.slice(sigStart);
    bodyText = lines.slice(0, sigStart).join("\n").trimEnd();
    const sigEl = $("letter-sig");
    sigEl.textContent = sigLines.join("\n");
    sigEl.style.display = "block";
  }

  const pre = $("letter-text");
  pre.textContent = "";
  typewrite(pre, bodyText);
}

function typewrite(el: HTMLElement, text: string) {
  const lines = text.split("\n");
  let i = 0;
  let lineIdx = 0;
  let charIdx = 0;

  function step() {
    if (lineIdx >= lines.length) return;
    const line = lines[lineIdx];
    if (charIdx < line.length) {
      el.textContent += line[charIdx];
      charIdx++;
      i++;
      setTimeout(step, i < 60 ? 20 : 12);
    } else {
      el.textContent += "\n";
      lineIdx++;
      charIdx = 0;
      setTimeout(step, lineIdx < lines.length && lines[lineIdx] === "" ? 70 : 28);
    }
  }
  step();
}

async function close() {
  if (!inTauri) return;
  const { getCurrentWebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  await getCurrentWebviewWindow().hide();
}

$("close-btn").addEventListener("click", close);
document.addEventListener("keydown", (e) => { if (e.key === "Escape") void close(); });

void init();
