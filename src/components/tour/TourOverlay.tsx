import { Check, ChevronRight } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { markOnboardingSeen, markTourCompleted, sendTourSignal, setTourActive } from "../../lib/tourStore";
import { STEPS, type TourStep } from "./steps";

type Box = { top: number; left: number; width: number; height: number };
type Placement = { left: number; top: number };

class Cancelled extends Error {}

const COACH_MAX_W = 344;
const EDGE = 16;
const GAP = 18;

/** Ширина карточки подсказки. На узком окне она сжимается, иначе уезжает за край. */
function coachWidth(): number {
  return Math.min(COACH_MAX_W, Math.max(240, window.innerWidth - EDGE * 2));
}

function anchorOf(name: string): HTMLElement | null {
  return document.querySelector<HTMLElement>(`[data-tour="${name}"]`);
}

/** A page may still be fetching when its step opens — the catalog feed on a
 *  cold start, for one — so give the anchor a moment to appear. */
async function waitForAnchor(name: string, sleep: (ms: number) => Promise<void>) {
  for (let attempt = 0; attempt < 24; attempt++) {
    const el = anchorOf(name);
    if (el) return el;
    await sleep(250);
  }
  return null;
}

function spotBox(el: HTMLElement, pad: number): Box {
  const r = el.getBoundingClientRect();
  const margin = 12;
  // Обрезаем рамку по краю экрана, а не двигаем её. Сдвиг был исходной причиной
  // кривой подсветки: у элемента, который не помещается целиком, рамка уезжала
  // на пустое место рядом. Обрезанная рамка всегда лежит на самом элементе.
  const left = Math.max(margin, r.left - pad);
  const top = Math.max(margin, r.top - pad);
  const right = Math.min(window.innerWidth - margin, r.right + pad);
  const bottom = Math.min(window.innerHeight - margin, r.bottom + pad);
  // Округляем: дробные пиксели от прокрутки заставляли бы рамку дрожать.
  return {
    top: Math.round(top),
    left: Math.round(left),
    width: Math.max(0, Math.round(right - left)),
    height: Math.max(0, Math.round(bottom - top)),
  };
}

/** Дождаться, пока плавная прокрутка встанет. */
async function waitForScrollSettled(
  el: HTMLElement,
  wait: (ms: number) => Promise<void>,
) {
  let previous = Number.NaN;
  let steady = 0;
  // Фиксированная пауза здесь и была причиной «иногда»: на медленной машине или
  // длинной прокрутке замер приходился на середину движения. Ждём остановки, но
  // не дольше секунды — застрявшая прокрутка не должна вешать обучение.
  for (let attempt = 0; attempt < 50; attempt++) {
    const top = el.getBoundingClientRect().top;
    if (Math.abs(top - previous) < 0.5) {
      if (++steady >= 2) return;
    } else {
      steady = 0;
    }
    previous = top;
    await wait(20);
  }
}

/** Pick a spot for the card that does not cover the thing it describes. */
function coachPlacement(box: Box, prefer: TourStep["prefer"], coachH: number): Placement {
  const W = window.innerWidth;
  const H = window.innerHeight;
  const COACH_W = coachWidth();
  const at: Record<string, Placement> = {
    right: { left: box.left + box.width + GAP, top: box.top + box.height / 2 - coachH / 2 },
    left: { left: box.left - COACH_W - GAP, top: box.top + box.height / 2 - coachH / 2 },
    below: { left: box.left + box.width / 2 - COACH_W / 2, top: box.top + box.height + GAP },
    above: { left: box.left + box.width / 2 - COACH_W / 2, top: box.top - coachH - GAP },
    br: { left: W - COACH_W - EDGE, top: H - coachH - EDGE },
    bl: { left: EDGE, top: H - coachH - EDGE },
    tr: { left: W - COACH_W - EDGE, top: EDGE },
    tl: { left: EDGE, top: EDGE },
  };
  const clamp = (p: Placement): Placement => ({
    left: Math.max(EDGE, Math.min(W - COACH_W - EDGE, p.left)),
    top: Math.max(EDGE, Math.min(H - coachH - EDGE, p.top)),
  });
  const covered = (p: Placement) => {
    const ow = Math.min(box.left + box.width, p.left + COACH_W) - Math.max(box.left, p.left);
    const oh = Math.min(box.top + box.height, p.top + coachH) - Math.max(box.top, p.top);
    return ow > 0 && oh > 0 ? ow * oh : 0;
  };
  const order = [...new Set([prefer, "right", "left", "below", "above", "br", "bl", "tr", "tl"])];
  let best: Placement | null = null;
  let bestCost = Infinity;
  for (const key of order) {
    const p = clamp(at[key as string]);
    const cost = covered(p);
    if (cost === 0) return p;
    if (cost < bestCost) {
      best = p;
      bestCost = cost;
    }
  }
  // None of the usual sides was free. Sweep the window for any gap that clears
  // the highlight rather than settling for "least bad" and sitting on top of it.
  const stepPx = 24;
  for (let top = EDGE; top + coachH <= H - EDGE; top += stepPx) {
    for (let left = EDGE; left + COACH_W <= W - EDGE; left += stepPx) {
      const p = { left, top };
      if (covered(p) === 0) return p;
    }
  }
  return best ?? clamp(at.below);
}

export default function TourOverlay({ onClose }: { onClose: () => void }) {
  const navigate = useNavigate();
  const [index, setIndex] = useState(0);
  const [box, setBox] = useState<Box | null>(null);
  const [place, setPlace] = useState<Placement>({ left: EDGE, top: EDGE });
  const [playing, setPlaying] = useState(false);
  const [celebrating, setCelebrating] = useState(false);
  const [ghost, setGhost] = useState<{ left: number; top: number; tap: boolean } | null>(null);
  const coachRef = useRef<HTMLDivElement>(null);
  const abortRef = useRef<Array<() => void>>([]);
  const doneRef = useRef(false);

  const reduce =
    typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;

  const abortAll = useCallback(() => {
    const list = abortRef.current;
    abortRef.current = [];
    for (const fn of list) fn();
  }, []);

  const wait = useCallback(
    (ms: number) =>
      new Promise<void>((resolve, reject) => {
        const id = window.setTimeout(resolve, reduce ? Math.min(ms, 50) : ms);
        abortRef.current.push(() => {
          window.clearTimeout(id);
          reject(new Cancelled());
        });
      }),
    [reduce],
  );

  const ghostTo = useCallback(
    async (name: string) => {
      const el = anchorOf(name);
      if (!el) return;
      const r = el.getBoundingClientRect();
      setGhost({ left: r.left + r.width / 2 - 10, top: r.top + r.height / 2 - 10, tap: false });
      await wait(620);
      setGhost(g => (g ? { ...g, tap: true } : g));
      await wait(420);
      setGhost(null);
    },
    [wait],
  );

  /** `celebrate` marks the tour as finished rather than abandoned. */
  const finish = useCallback(
    (celebrate: boolean) => {
      if (doneRef.current) return;
      abortAll();
      sendTourSignal("");
      setTourActive(false);
      markOnboardingSeen();
      if (celebrate) {
        markTourCompleted();
        setCelebrating(true);
        return;
      }
      doneRef.current = true;
      onClose();
    },
    [abortAll, onClose],
  );

  const closeCelebration = useCallback(() => {
    doneRef.current = true;
    navigate("/");
    onClose();
  }, [navigate, onClose]);

  const go = useCallback(
    async (next: number, dir: 1 | -1) => {
      if (doneRef.current) return;
      if (next >= STEPS.length) {
        finish(true);
        return;
      }
      if (next < 0) return;
      abortAll();
      setIndex(next);
      const step = STEPS[next];
      setGhost(null);

      try {
        if (dir > 0 && step.anim) {
          setPlaying(true);
          await step.anim({ wait, ghostTo, navigate, signal: sendTourSignal });
        }
        step.state({ navigate, signal: sendTourSignal });
        // Let the page render the state we just asked for.
        await wait(reduce ? 0 : 60);
        const el = await waitForAnchor(step.anchor, wait);
        if (!el) {
          setPlaying(false);
          return;
        }
        // A target that is wide or tall leaves no room beside it, so it gets
        // pinned to the top and the card takes the band underneath. Centring
        // those would trap the card with nowhere free to sit.
        const rect = el.getBoundingClientRect();
        const needsBand =
          rect.width > window.innerWidth * 0.55 || rect.height > window.innerHeight * 0.35;
        el.scrollIntoView({
          block: needsBand ? "start" : "center",
          behavior: reduce ? "auto" : "smooth",
        });
        if (!reduce) await waitForScrollSettled(el, wait);
        const target = anchorOf(step.anchor);
        if (!target) {
          setPlaying(false);
          return;
        }
        setBox(spotBox(target, step.pad ?? 8));
        setPlaying(false);
      } catch (error) {
        if (!(error instanceof Cancelled)) throw error;
      }
    },
    [abortAll, finish, ghostTo, navigate, reduce, wait],
  );

  // Position the card once its height is known, and keep both in step with
  // window resizes — the app window is freely resizable.
  useEffect(() => {
    if (!box) return;
    const h = coachRef.current?.offsetHeight ?? 210;
    setPlace(coachPlacement(box, STEPS[index].prefer, h));
  }, [box, index]);

  useEffect(() => {
    setTourActive(true);
    void go(0, 1);
    return () => {
      setTourActive(false);
      sendTourSignal("");
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (playing) return;
    let frame = 0;
    let previous = "";
    const track = () => {
      const el = anchorOf(STEPS[index].anchor);
      if (el) {
        const next = spotBox(el, STEPS[index].pad ?? 8);
        const key = `${next.top}|${next.left}|${next.width}|${next.height}`;
        if (key !== previous) {
          previous = key;
          setBox(next);
        }
      }
      frame = window.requestAnimationFrame(track);
    };
    frame = window.requestAnimationFrame(track);
    return () => window.cancelAnimationFrame(frame);
  }, [index, playing]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "ArrowRight") void go(index + 1, 1);
      else if (e.key === "ArrowLeft") void go(index - 1, -1);
      else if (e.key === "Escape") finish(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [finish, go, index]);

  const step = STEPS[index];
  const isLast = index === STEPS.length - 1;

  if (celebrating) {
    return (
      <div className="tour-done" role="dialog" aria-modal="true">
        <div className="tour-done-card">
          <div className="tour-done-mark">
            <Check size={32} strokeWidth={2.6} />
          </div>
          <h3>Готово — вы научились пользоваться Signum</h3>
          <button type="button" className="btn-primary tour-done-btn" onClick={closeCelebration}>
            Перейти в Каталог
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="tour-root" role="dialog" aria-modal="true" aria-label="Обучение">
      {/* Swallows clicks on the app underneath: a stray tap on a real catalog
          card during the tour would fire a real App Store request. */}
      <div className="tour-block" aria-hidden />
      <div
        className={`tour-spot ${playing || !box ? "is-hidden" : ""}`}
        style={
          box
            ? {
                top: box.top,
                left: box.left,
                width: box.width,
                height: box.height,
                borderRadius: step.radius ?? Math.min(20, 8 + (step.pad ?? 8)),
              }
            : undefined
        }
        aria-hidden
      />

      {ghost && (
        <div
          className={`tour-ghost ${ghost.tap ? "is-tap" : ""}`}
          style={{ left: ghost.left, top: ghost.top }}
          aria-hidden
        />
      )}

      <div ref={coachRef} className="tour-coach" style={{ left: place.left, top: place.top }}>
        {/* Skip lives inside the card. Floating in a window corner it landed
            on top of whatever was being highlighted. */}
        <div className="tour-head">
          <span className="tour-eyebrow">{step.eyebrow}</span>
          <button type="button" className="tour-skip" onClick={() => finish(false)}>
            Пропустить
          </button>
        </div>
        <h4 className="tour-title">{step.title}</h4>
        <p className="tour-text">{step.text}</p>

        {/* A bar, not 16 dashes: at this length dashes overflowed the card and
            pushed the buttons outside it. */}
        <div className="tour-progress" aria-hidden>
          <i style={{ width: `${((index + 1) / STEPS.length) * 100}%` }} />
        </div>
        <div className="tour-row">
          <span className="tour-count">
            {index + 1} из {STEPS.length}
          </span>
          <div className="tour-actions">
            {index > 0 && (
              <button type="button" className="btn-secondary tour-btn" onClick={() => void go(index - 1, -1)}>
                Назад
              </button>
            )}
            <button type="button" className="btn-primary tour-btn" onClick={() => void go(index + 1, 1)}>
              {isLast ? "Завершить" : "Далее"}
              {isLast ? <Check size={15} strokeWidth={2.4} /> : <ChevronRight size={15} strokeWidth={2.4} />}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
