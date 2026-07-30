import { ShieldCheck } from 'lucide-react';

export function AppLogo({
  compact = false,
  inverted = false,
}: {
  compact?: boolean;
  inverted?: boolean;
}) {
  return (
    <div className="flex items-center gap-3" aria-label="Offline Dental System">
      <span className="grid size-11 shrink-0 place-items-center rounded-xl bg-petrol-700 text-white shadow-sm">
        <ShieldCheck className="size-6" strokeWidth={1.8} aria-hidden="true" />
      </span>
      {compact ? null : (
        <span>
          <span
            className={`block text-sm font-bold tracking-tight ${inverted ? 'text-white' : 'text-slate-950'}`}
          >
            Offline Dental
          </span>
          <span
            className={`block text-xs font-medium ${inverted ? 'text-petrol-200' : 'text-petrol-700'}`}
          >
            System
          </span>
        </span>
      )}
    </div>
  );
}
