import { AlertCircle, LoaderCircle } from "lucide-react";

export default function StatusMessage({ loading, error, children }: { loading?: boolean; error?: string; children?: React.ReactNode }) {
  return (
    <div className="panel flex min-h-52 flex-col items-center justify-center gap-3 p-8 text-center">
      {loading ? <LoaderCircle className="animate-spin text-brand-400" /> : <AlertCircle className="text-rose-400" />}
      <p role={error ? "alert" : undefined} className={`text-sm ${error ? "text-rose-300" : "text-slate-400"}`}>{error ?? children ?? "Загрузка…"}</p>
    </div>
  );
}
