import { LoaderCircle } from 'lucide-react';
import type { ButtonHTMLAttributes, ReactNode } from 'react';

import { cn } from './cn';

type ButtonVariant = 'primary' | 'secondary' | 'ghost' | 'danger';

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  busy?: boolean;
  icon?: ReactNode;
}

const variants: Record<ButtonVariant, string> = {
  primary: 'bg-petrol-700 text-white shadow-sm hover:bg-petrol-800 disabled:bg-slate-300',
  secondary:
    'border border-slate-300 bg-white text-slate-800 hover:border-petrol-300 hover:bg-petrol-50 disabled:bg-slate-50',
  ghost: 'text-slate-700 hover:bg-slate-100 disabled:text-slate-400',
  danger: 'bg-red-700 text-white hover:bg-red-800 disabled:bg-slate-300',
};

export function Button({
  children,
  className,
  variant = 'primary',
  busy = false,
  icon,
  disabled,
  type = 'button',
  ...props
}: ButtonProps) {
  return (
    <button
      type={type}
      className={cn(
        'inline-flex min-h-11 items-center justify-center gap-2 rounded-lg px-4 py-2.5 text-sm font-semibold transition-colors',
        'focus-visible:outline-3 focus-visible:outline-offset-2 focus-visible:outline-petrol-500',
        'disabled:cursor-not-allowed disabled:opacity-70',
        variants[variant],
        className,
      )}
      disabled={disabled || busy}
      aria-busy={busy}
      {...props}
    >
      {busy ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : icon}
      {children}
    </button>
  );
}
