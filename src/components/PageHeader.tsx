export default function PageHeader({ title, subtitle, actions }: { title: string; subtitle: string; actions?: React.ReactNode }) {
  return (
    <div className="flex items-start justify-between gap-6">
      <div><h1 className="page-title">{title}</h1><p className="mt-1 text-sm text-slate-500">{subtitle}</p></div>
      {actions && <div className="flex items-center gap-2">{actions}</div>}
    </div>
  );
}
