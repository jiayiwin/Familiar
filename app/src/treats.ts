// The sweets and fruit the pets eat, drawn. Each kind in treats.yaml names a
// shape and two colours; this draws it in the pets' own manner: flat soft
// fills, one warm line, a small light.

export type TreatShape = "wrapped" | "drop" | "bean" | "star" | "cloud" | "lolly" | "cube" | "moon" | "slice" | "berry" | "grape" | "cherry" | "peach" | "leaf" | "flower";

export interface TreatLook {
  shape?: string;
  colors?: string[];
}

const TAU = Math.PI * 2;

/** `hex` moved toward black (k < 0) or white (k > 0). */
function tone(hex: string, k: number): string {
  const n = parseInt(hex.replace("#", ""), 16);
  const to = k < 0 ? 0 : 255;
  const f = Math.abs(k);
  const ch = (v: number) => Math.round(v + (to - v) * f);
  return `rgb(${ch((n >> 16) & 255)},${ch((n >> 8) & 255)},${ch(n & 255)})`;
}

/**
 * Draw one treat centred on (cx, cy); it fits a box about 40 × `size` across.
 * `locked`: one not met yet, as a pale shape with no colour.
 */
export function drawTreat(g: CanvasRenderingContext2D, look: TreatLook, cx: number, cy: number, size = 1, locked = false): void {
  const [c0, c1] = locked ? ["#d9d3de", "#c4bdcb"] : [look.colors?.[0] ?? "#f6b45e", look.colors?.[1] ?? "#fbe2b0"];
  const line = locked ? "rgba(150,140,160,.7)" : tone(c0, -0.42);
  g.save();
  g.translate(cx, cy);
  g.scale(size, size);
  g.lineJoin = g.lineCap = "round";
  g.lineWidth = 1.5;
  g.strokeStyle = line;
  const fill = (c: string) => {
    g.fillStyle = c;
    g.fill();
  };
  const both = (c: string) => {
    fill(c);
    g.stroke();
  };
  const circle = (x: number, y: number, r: number) => {
    g.beginPath();
    g.arc(x, y, r, 0, TAU);
  };
  const glint = (x: number, y: number, rx: number, ry: number, rot = -0.5) => {
    g.beginPath();
    g.ellipse(x, y, rx, ry, rot, 0, TAU);
    fill("rgba(255,255,255,.62)");
  };
  switch (look.shape) {
    case "wrapped": {
      // a twist of paper at each end
      for (const s of [-1, 1]) {
        g.beginPath();
        g.moveTo(s * 9, -3);
        g.lineTo(s * 19, -8);
        g.lineTo(s * 16.5, 0);
        g.lineTo(s * 19, 8);
        g.lineTo(s * 9, 3);
        g.closePath();
        both(c1);
      }
      g.beginPath();
      g.ellipse(0, 0, 11, 8, 0, 0, TAU);
      both(c0);
      g.save();
      g.clip();
      g.strokeStyle = c1;
      g.lineWidth = 2.6;
      for (const x of [-5, 1, 7]) {
        g.beginPath();
        g.moveTo(x - 3, -9);
        g.lineTo(x + 2, 9);
        g.stroke();
      }
      g.restore();
      glint(-4, -3.5, 3.4, 1.6);
      break;
    }
    case "drop": {
      circle(0, 1, 11.5);
      both(c0);
      g.beginPath();
      g.arc(0, 1, 6.5, Math.PI * 0.2, Math.PI * 1.25);
      g.strokeStyle = c1;
      g.lineWidth = 2.6;
      g.stroke();
      glint(-4.5, -4, 3.2, 1.7);
      // what flies off it
      g.fillStyle = c1;
      for (const [x, y, r] of [[13, -9, 1.9], [16, -2, 1.3], [-14, -10, 1.5]]) {
        circle(x, y, r);
        g.fill();
      }
      break;
    }
    case "bean": {
      g.rotate(-0.45);
      g.beginPath();
      g.moveTo(-11, -1);
      g.bezierCurveTo(-13, -10, 13, -11, 12, 0);
      g.bezierCurveTo(12, 9, 4, 9, 0, 4.5);
      g.bezierCurveTo(-4, 9, -10, 8, -11, -1);
      g.closePath();
      both(c0);
      glint(-3, -4.5, 4, 1.7, 0.1);
      circle(8, 3, 1.6);
      fill(c1);
      break;
    }
    case "star": {
      // sugar with little horns all round, and a small one beside it
      const horned = (x: number, y: number, r: number, c: string) => {
        g.beginPath();
        for (let i = 0; i < 16; i++) {
          const a = (i / 16) * TAU;
          const rr = i % 2 ? r * 0.74 : r;
          g.lineTo(x + Math.cos(a) * rr, y + Math.sin(a) * rr);
        }
        g.closePath();
        both(c);
      };
      horned(-3, 0, 11, c0);
      horned(11, 7, 5.5, c1);
      glint(-6.5, -4, 2.8, 1.5);
      break;
    }
    case "cloud": {
      g.beginPath();
      g.moveTo(0, 6);
      g.lineTo(2, 19);
      g.lineWidth = 2.4;
      g.strokeStyle = tone(c0, -0.5);
      g.stroke();
      g.lineWidth = 1.5;
      g.strokeStyle = line;
      g.beginPath();
      g.arc(-7, 0, 7, Math.PI * 0.5, Math.PI * 1.5);
      g.arc(-2, -7, 7, Math.PI, Math.PI * 1.9);
      g.arc(7, -3, 7.5, Math.PI * 1.35, Math.PI * 0.35);
      g.arc(1, 5, 7, 0, Math.PI * 0.9);
      g.closePath();
      both(c0);
      g.beginPath();
      g.arc(1, -1, 5, Math.PI * 0.9, Math.PI * 2.1);
      g.strokeStyle = c1;
      g.lineWidth = 2.4;
      g.stroke();
      break;
    }
    case "lolly": {
      g.beginPath();
      g.moveTo(0, 8);
      g.lineTo(0, 19);
      g.lineWidth = 2.4;
      g.strokeStyle = tone(c0, -0.5);
      g.stroke();
      g.lineWidth = 1.5;
      g.strokeStyle = line;
      circle(0, -3, 11.5);
      both(c0);
      g.beginPath();
      for (let i = 0; i <= 40; i++) {
        const a = (i / 40) * TAU * 2.25;
        const r = 0.8 + (i / 40) * 9.4;
        g.lineTo(Math.cos(a) * r, -3 + Math.sin(a) * r);
      }
      g.strokeStyle = c1;
      g.lineWidth = 2.6;
      g.stroke();
      glint(-5, -8, 2.8, 1.4);
      break;
    }
    case "cube": {
      const jelly = (x: number, y: number, s: number, rot: number, c: string) => {
        g.save();
        g.translate(x, y);
        g.rotate(rot);
        g.beginPath();
        g.roundRect(-s, -s, s * 2, s * 2, s * 0.42);
        both(c);
        glint(-s * 0.35, -s * 0.45, s * 0.42, s * 0.2, 0);
        g.restore();
      };
      jelly(6, 3, 8, 0.22, c1);
      jelly(-5, -2, 8.5, -0.18, c0);
      break;
    }
    case "moon": {
      g.beginPath();
      g.arc(0, 0, 12, Math.PI * 0.32, Math.PI * 1.68, false);
      g.arc(6.5, 0, 9.2, Math.PI * 1.42, Math.PI * 0.58, true);
      g.closePath();
      both(c0);
      glint(-7.5, -4.5, 2.2, 1.2, 1);
      g.fillStyle = c1;
      for (const [x, y, r] of [[12, -9, 1.6], [15, 2, 1.1]]) {
        circle(x, y, r);
        g.fill();
      }
      break;
    }
    case "slice": {
      // a wedge: rind outside, flesh inside
      g.beginPath();
      g.arc(0, -5, 15, 0, Math.PI);
      g.closePath();
      both(c1);
      g.beginPath();
      g.arc(0, -5, 11.5, 0, Math.PI);
      g.closePath();
      fill(c0);
      g.strokeStyle = tone(c0, 0.45);
      g.lineWidth = 1.3;
      for (const a of [0.25, 0.5, 0.75]) {
        g.beginPath();
        g.moveTo(0, -5);
        g.lineTo(Math.cos(Math.PI * a) * 10, -5 + Math.sin(Math.PI * a) * 10);
        g.stroke();
      }
      break;
    }
    case "berry": {
      g.beginPath();
      g.moveTo(0, 13);
      g.bezierCurveTo(-14, 5, -13, -8, -5, -8);
      g.bezierCurveTo(-2, -8, 2, -8, 5, -8);
      g.bezierCurveTo(13, -8, 14, 5, 0, 13);
      g.closePath();
      both(c0);
      g.fillStyle = "rgba(255,246,214,.9)";
      for (const [x, y] of [[-5, -2], [1, -3], [6, -1], [-2, 3], [4, 4], [0, 8]]) {
        g.beginPath();
        g.ellipse(x, y, 0.9, 1.4, 0, 0, TAU);
        g.fill();
      }
      // the leaves on top
      g.beginPath();
      g.moveTo(-8, -8);
      g.lineTo(-3, -11.5);
      g.lineTo(0, -8);
      g.lineTo(3, -11.5);
      g.lineTo(8, -8);
      g.lineTo(0, -5.5);
      g.closePath();
      g.strokeStyle = tone(c1, -0.4);
      both(c1);
      break;
    }
    case "grape": {
      for (const [x, y] of [[-7, -3], [0, -4], [7, -3], [-3.5, 3], [3.5, 3], [0, 9.5]]) {
        circle(x, y, 5.2);
        both(c0);
        glint(x - 1.6, y - 1.8, 1.4, 0.8);
      }
      g.beginPath();
      g.moveTo(0, -8);
      g.lineTo(1, -14);
      g.strokeStyle = tone(c1, -0.45);
      g.stroke();
      g.beginPath();
      g.ellipse(6, -12, 5, 2.6, -0.4, 0, TAU);
      both(c1);
      break;
    }
    case "cherry": {
      g.beginPath();
      g.moveTo(-6, 2);
      g.quadraticCurveTo(-3, -9, 2, -13);
      g.quadraticCurveTo(5, -6, 8, 4);
      g.strokeStyle = tone(c1, -0.45);
      g.stroke();
      g.beginPath();
      g.ellipse(7, -13, 5.5, 2.6, 0.35, 0, TAU);
      both(c1);
      g.strokeStyle = line;
      for (const [x, y] of [[-7, 6], [8, 8]]) {
        circle(x, y, 6.6);
        both(c0);
        glint(x - 2.2, y - 2.4, 1.9, 1);
      }
      break;
    }
    case "peach": {
      circle(0, 2, 11.5);
      both(c0);
      g.beginPath();
      g.moveTo(1, -9);
      g.quadraticCurveTo(-4, 1, 1, 12.5);
      g.strokeStyle = tone(c0, -0.24);
      g.stroke();
      glint(-5, -3, 3, 1.6);
      g.beginPath();
      g.ellipse(6, -11, 5.5, 2.7, -0.5, 0, TAU);
      g.strokeStyle = tone(c1, -0.4);
      both(c1);
      break;
    }
    case "leaf": {
      g.rotate(-0.5);
      g.beginPath();
      g.moveTo(-15, 0);
      g.bezierCurveTo(-7, -11, 8, -10, 15, 0);
      g.bezierCurveTo(8, 10, -7, 11, -15, 0);
      g.closePath();
      both(c0);
      g.strokeStyle = c1;
      g.beginPath();
      g.moveTo(-13, 0);
      g.lineTo(13, 0);
      for (const x of [-6, 0, 6]) {
        g.moveTo(x, 0);
        g.lineTo(x + 4, -4.5);
        g.moveTo(x, 0);
        g.lineTo(x + 4, 4.5);
      }
      g.stroke();
      break;
    }
    case "flower": {
      for (let i = 0; i < 5; i++) {
        g.save();
        g.rotate((i / 5) * TAU);
        g.beginPath();
        g.moveTo(0, -1);
        g.bezierCurveTo(-7, -6, -5, -14, 0, -12);
        g.bezierCurveTo(5, -14, 7, -6, 0, -1);
        g.closePath();
        both(c0);
        g.restore();
      }
      circle(0, 0, 3.2);
      both(c1);
      break;
    }
    default: {
      circle(0, 0, 11);
      both(c0);
      glint(-4, -4, 3, 1.6);
    }
  }
  g.restore();
}

/** When a kind turns up, in a few words, for one not met yet; null for the everyday ones. */
export function whenOf(f: { from?: string | null; hours?: [number, number] | null; months?: [number, number] | null }, t: (zh: string, en: string) => string): string | null {
  if (f.from === "agent") return t("agent 做完一轮才有", "from an agent's finished turn");
  if (f.from === "kegel") return t("跟它做完一组提肛", "for a kegel set done with it");
  if (f.from === "break") return t("久坐后起身歇一会儿", "for getting up after a long sit");
  if (f.hours) {
    const [a] = f.hours;
    return a >= 20 || a < 4 ? t("夜里才有", "only at night") : a < 11 ? t("早上才有", "only in the morning") : t("午后才有", "only in the afternoon");
  }
  if (f.months) {
    const [a] = f.months;
    return a >= 12 || a <= 2 ? t("冬天才有", "only in winter") : a <= 5 ? t("春天才有", "only in spring") : a <= 8 ? t("夏天才有", "only in summer") : t("秋天才有", "only in autumn");
  }
  return null;
}
