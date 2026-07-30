import { AlertCircle, CircleCheck, Info, TriangleAlert } from 'lucide-react';
import type { HTMLAttributes, ReactNode } from 'react';

import { cn } from './cn';

type AlertVariant = 'info' | 'success' | 'warning' | 'danger';

const styles: Record<AlertVariant, string> = {
  info: 'border-petrol-200 bg-petrol-50 text-petrol-950',
  success: 'border-emerald-200 bg-emerald-50 text-emerald-950',
  warning: 'border-amber-200 bg-amber-50 text-amber-950',
  danger: 'border-red-200 bg-red-50 text-red-950',
};

const icons: Record<AlertVariant, ReactNode> = {
  info: <Info className="size-5" aria-hidden="true" />,
  success: <CircleCheck className="size-5" aria-hidden="true" />,
  warning: <TriangleAlert className="size-5" aria-hidden="true" />,
  danger: <AlertCircle className="size-5" aria-hidden="true" />,
};

export interface AlertProps extends HTMLAttributes<HTMLDivElement> {
  variant?: AlertVariant;
  title?: string;
}

export function Alert({
  variant = 'info',
  title,
  children,
  className,
  ...props
}: AlertProps) {
  return (
    <div
      role={variant === 'danger' ? 'alert' : 'status'}
      className={cn(
        'flex gap-3 rounded-lg border p-4 text-sm',
        styles[variant],
        className,
      )}
      {...props}
    >
      <span className="mt-0.5 shrink-0">{icons[variant]}</span>
      <div className="min-w-0">
        {title ? <p className="mb-1 font-semibold">{title}</p> : null}
        <div className="leading-6">{children}</div>
      </div>
    </div>
  );
}
