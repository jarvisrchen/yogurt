import { useMemo } from "react";
import QRCode from "qrcode";

/** The white square stays white in dark mode, because a scanner needs dark modules on a light ground. */
export function QrCode({ text, label }: { text: string; label: string }) {
  const { size, path } = useMemo(() => {
    const qr = QRCode.create(text, { errorCorrectionLevel: "M" });
    const n = qr.modules.size;
    let d = "";
    for (let y = 0; y < n; y++)
      for (let x = 0; x < n; x++) if (qr.modules.get(y, x)) d += `M${x} ${y}h1v1h-1z`;
    return { size: n, path: d };
  }, [text]);
  return (
    <svg
      role="img"
      aria-label={label}
      viewBox={`0 0 ${size} ${size}`}
      shapeRendering="crispEdges"
      className="size-24 shrink-0 rounded-[10px] border border-line bg-white p-1.5"
    >
      <path d={path} fill="#000" />
    </svg>
  );
}
