import { useEffect, useState } from "react";

/**
 * Подпись под крутящимся индикатором, которая проходит по этапам.
 *
 * Этапы — не украшение: это то, что приложение действительно делает по порядку
 * (открывает карточку, спрашивает Apple, разбирает список версий). Дойдя до
 * последнего, подпись на нём и останавливается — бесконечный круг обещал бы
 * движение там, где его уже нет.
 */
export default function LoadingSteps({ steps, intervalMs = 2200 }: {
  steps: string[];
  intervalMs?: number;
}) {
  const [index, setIndex] = useState(0);
  const key = steps.join("|");

  useEffect(() => {
    setIndex(0);
    if (steps.length < 2) return;
    const timer = window.setInterval(() => {
      setIndex(current => (current + 1 < steps.length ? current + 1 : current));
    }, intervalMs);
    return () => window.clearInterval(timer);
                                                                            
  }, [key, intervalMs, steps.length]);

  const text = steps[Math.min(index, steps.length - 1)] ?? "";
  // key по тексту — чтобы React заменил узел и анимация проигралась заново.
  return <span key={text} className="signum-loading-step">{text}</span>;
}
