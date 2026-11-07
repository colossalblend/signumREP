import { getCurrentWindow } from "@tauri-apps/api/window";
import type { PointerEvent, ReactNode } from "react";

   
                                                              
                                                                               
                                                                     
   
export default function DragRegion({ className, children }: { className?: string; children?: ReactNode }) {
  function startDrag(event: PointerEvent<HTMLDivElement>) {
    if (event.button !== 0) return;
    const target = event.target as HTMLElement | null;
    if (!target) return;
                                                  
    if (target.closest("button, a, input, textarea, select, label, [data-no-drag]")) return;
                                                   
    if (event.pointerType === "touch" && !event.isPrimary) return;

                                                                                       
    void getCurrentWindow().startDragging().catch(() => undefined);
  }

  return (
    <div
      className={`signum-drag-region ${className ?? ""}`.trim()}
      onPointerDown={startDrag}
      data-tauri-drag-region
    >
      {children}
    </div>
  );
}
