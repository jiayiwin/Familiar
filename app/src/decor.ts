// Head-top cosmetics: season crown / loser hat and shop hats. Drawn in code
// so they fit any pack: `cx` / `top` is the top-centre of the pet's head and
// `w` its width at that height (all in canvas px).

import { t } from "./i18n";

export const HAT_NAMES: Record<string, string> = {
  get crown() { return t("赛季皇冠", "Season crown"); },
  get loser_hat() { return t("纸帽", "Paper hat"); },
  get flower() { return t("小红花", "Red flower"); },
  get bow() { return t("蝴蝶结", "Bow"); },
  get straw_hat() { return t("草帽", "Straw hat"); },
  get party_hat() { return t("派对帽", "Party hat"); },
  get beanie() { return t("毛线帽", "Beanie"); },
  get wizard_hat() { return t("法师帽", "Wizard hat"); },
  get halo() { return t("光环", "Halo"); },
};

const OUT = "#3a2418";

function outline(c: CanvasRenderingContext2D, w: number) {
  c.lineWidth = Math.max(1.5, w * 0.05);
  c.strokeStyle = OUT;
  c.stroke();
}

export function drawHat(c: CanvasRenderingContext2D, id: string, cx: number, top: number, w: number, now = 0): void {
  const u = Math.max(8, w); // head width
  c.save();
  c.lineJoin = "round";
  switch (id) {
    case "crown": {
      const hw = u * 0.42, h = u * 0.32, y = top + u * 0.06;
      c.beginPath();
      c.moveTo(cx - hw, y);
      c.lineTo(cx - hw, y - h * 0.55);
      c.lineTo(cx - hw * 0.5, y - h * 0.25);
      c.lineTo(cx, y - h);
      c.lineTo(cx + hw * 0.5, y - h * 0.25);
      c.lineTo(cx + hw, y - h * 0.55);
      c.lineTo(cx + hw, y);
      c.closePath();
      c.fillStyle = "#f4c430";
      c.fill();
      outline(c, u);
      for (const [dx, col] of [[-hw * 0.5, "#e63946"], [0, "#3f8efc"], [hw * 0.5, "#2a9d8f"]] as const) {
        c.beginPath();
        c.arc(cx + dx, y - h * 0.18, u * 0.045, 0, Math.PI * 2);
        c.fillStyle = col;
        c.fill();
      }
      // Sparkle so champions stand out.
      const t = (now / 400) % (Math.PI * 2);
      c.globalAlpha = 0.5 + 0.5 * Math.sin(t);
      c.fillStyle = "#fff6c2";
      c.fillRect(cx + hw * 0.7, y - h * 1.05, u * 0.05, u * 0.05);
      break;
    }
    case "loser_hat": {
      // Dunce cap, tilted.
      c.translate(cx, top + u * 0.05);
      c.rotate(0.25);
      c.beginPath();
      c.moveTo(-u * 0.2, 0);
      c.lineTo(u * 0.02, -u * 0.55);
      c.lineTo(u * 0.22, 0);
      c.closePath();
      c.fillStyle = "#e9e4da";
      c.fill();
      outline(c, u);
      c.fillStyle = OUT;
      c.font = `bold ${Math.round(u * 0.14)}px system-ui, sans-serif`;
      c.textAlign = "center";
      c.fillText(t("输", "L"), u * 0.01, -u * 0.12);
      break;
    }
    case "flower": {
      const x = cx + u * 0.28, y = top + u * 0.08, r = u * 0.08;
      for (let i = 0; i < 5; i++) {
        const a = (i / 5) * Math.PI * 2;
        c.beginPath();
        c.arc(x + Math.cos(a) * r, y + Math.sin(a) * r, r * 0.9, 0, Math.PI * 2);
        c.fillStyle = "#e63946";
        c.fill();
      }
      c.beginPath();
      c.arc(x, y, r * 0.7, 0, Math.PI * 2);
      c.fillStyle = "#ffd166";
      c.fill();
      break;
    }
    case "bow": {
      const x = cx, y = top + u * 0.04, s = u * 0.2;
      c.fillStyle = "#ff6fa8";
      for (const d of [-1, 1]) {
        c.beginPath();
        c.moveTo(x, y);
        c.lineTo(x + d * s, y - s * 0.6);
        c.lineTo(x + d * s, y + s * 0.6);
        c.closePath();
        c.fill();
        outline(c, u);
      }
      c.beginPath();
      c.arc(x, y, s * 0.28, 0, Math.PI * 2);
      c.fillStyle = "#ff3d8b";
      c.fill();
      outline(c, u);
      break;
    }
    case "straw_hat": {
      const y = top + u * 0.1;
      c.beginPath();
      c.ellipse(cx, y, u * 0.62, u * 0.13, 0, 0, Math.PI * 2);
      c.fillStyle = "#e9c46a";
      c.fill();
      outline(c, u);
      c.beginPath();
      c.ellipse(cx, y - u * 0.1, u * 0.32, u * 0.17, 0, Math.PI, 0);
      c.closePath();
      c.fillStyle = "#f2d57e";
      c.fill();
      outline(c, u);
      c.fillStyle = "#e63946";
      c.fillRect(cx - u * 0.32, y - u * 0.06, u * 0.64, u * 0.05);
      break;
    }
    case "party_hat": {
      const y = top + u * 0.05;
      c.beginPath();
      c.moveTo(cx - u * 0.2, y);
      c.lineTo(cx, y - u * 0.5);
      c.lineTo(cx + u * 0.2, y);
      c.closePath();
      c.fillStyle = "#7b6cd9";
      c.fill();
      outline(c, u);
      c.fillStyle = "#ffd166";
      for (let i = 1; i <= 3; i++) {
        c.beginPath();
        c.arc(cx + (i % 2 ? -1 : 1) * u * 0.05, y - u * 0.12 * i, u * 0.03, 0, Math.PI * 2);
        c.fill();
      }
      c.beginPath();
      c.arc(cx, y - u * 0.52, u * 0.06, 0, Math.PI * 2);
      c.fillStyle = "#ff6fa8";
      c.fill();
      break;
    }
    case "beanie": {
      const y = top + u * 0.14;
      c.beginPath();
      c.ellipse(cx, y, u * 0.4, u * 0.3, 0, Math.PI, 0);
      c.closePath();
      c.fillStyle = "#3f8efc";
      c.fill();
      outline(c, u);
      c.fillStyle = "#e8f0ff";
      c.fillRect(cx - u * 0.42, y - u * 0.04, u * 0.84, u * 0.1);
      c.beginPath();
      c.arc(cx, y - u * 0.32, u * 0.08, 0, Math.PI * 2);
      c.fillStyle = "#ffffff";
      c.fill();
      outline(c, u);
      break;
    }
    case "wizard_hat": {
      const y = top + u * 0.1;
      c.beginPath();
      c.ellipse(cx, y, u * 0.5, u * 0.1, 0, 0, Math.PI * 2);
      c.fillStyle = "#2b2d6e";
      c.fill();
      outline(c, u);
      c.beginPath();
      c.moveTo(cx - u * 0.26, y - u * 0.02);
      c.quadraticCurveTo(cx - u * 0.02, y - u * 0.5, cx + u * 0.22, y - u * 0.7);
      c.lineTo(cx + u * 0.26, y - u * 0.02);
      c.closePath();
      c.fill();
      outline(c, u);
      c.fillStyle = "#ffd166";
      c.font = `${Math.round(u * 0.16)}px system-ui, sans-serif`;
      c.textAlign = "center";
      c.fillText("★", cx, y - u * 0.18);
      break;
    }
    case "halo": {
      const y = top - u * 0.12 + Math.sin(now / 500) * u * 0.03;
      c.beginPath();
      c.ellipse(cx, y, u * 0.3, u * 0.08, 0, 0, Math.PI * 2);
      c.lineWidth = Math.max(2, u * 0.06);
      c.strokeStyle = "#ffd166";
      c.shadowColor = "#fff3b0";
      c.shadowBlur = u * 0.15;
      c.stroke();
      break;
    }
  }
  c.restore();
}

/** Rope colours for tug of war. */
export function ropeStyle(c: CanvasRenderingContext2D, rope: string | null | undefined, x0: number, x1: number): string | CanvasGradient {
  if (rope === "rope_gold") return "#f4c430";
  if (rope === "rope_rainbow") {
    const g = c.createLinearGradient(x0, 0, x1, 0);
    ["#e63946", "#f4a261", "#e9c46a", "#2a9d8f", "#3f8efc", "#7b6cd9"].forEach((col, i, a) => g.addColorStop(i / (a.length - 1), col));
    return g;
  }
  return "#a0703a";
}
