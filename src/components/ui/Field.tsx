import { Eye, EyeOff } from 'lucide-react';
import { forwardRef, useId, useState } from 'react';
import type { InputHTMLAttributes, ReactNode } from 'react';

import { cn } from './cn';

export interface FieldProps extends InputHTMLAttributes<HTMLInputElement> {
  label: string;
  error?: string | undefined;
  hint?: string | undefined;
  optional?: boolean;
  trailing?: ReactNode | undefined;
}

export const Field = forwardRef<HTMLInputElement, FieldProps>(function Field(
  { label, error, hint, optional, trailing, id, className, type = 'text', ...props },
  ref,
) {
  const generatedId = useId();
  const fieldId = id ?? generatedId;
  const descriptionId = `${fieldId}-description`;
  const [visible, setVisible] = useState(false);
  const isPassword = type === 'password';

  return (
    <div className="grid gap-1.5">
      <label className="text-sm font-semibold text-slate-800" htmlFor={fieldId}>
        {label}
        {optional ? (
          <span className="ml-1 font-normal text-slate-500">(opcional)</span>
        ) : null}
      </label>
      <div className="relative">
        <input
          ref={ref}
          id={fieldId}
          type={isPassword && visible ? 'text' : type}
          className={cn(
            'min-h-11 w-full rounded-lg border bg-white px-3 py-2.5 text-sm text-slate-950 outline-none transition',
            'placeholder:text-slate-400 focus:border-petrol-500 focus:ring-3 focus:ring-petrol-100',
            'disabled:cursor-not-allowed disabled:bg-slate-100 disabled:text-slate-500',
            error ? 'border-red-500' : 'border-slate-300',
            Boolean(isPassword || trailing) && 'pr-11',
            className,
          )}
          aria-invalid={Boolean(error)}
          aria-describedby={error || hint ? descriptionId : undefined}
          {...props}
        />
        {isPassword ? (
          <button
            type="button"
            className="absolute inset-y-0 right-0 grid w-11 place-items-center rounded-r-lg text-slate-500 hover:text-petrol-700 focus-visible:outline-3 focus-visible:outline-petrol-500"
            aria-label={visible ? 'Ocultar senha' : 'Mostrar senha'}
            onClick={() => setVisible((current) => !current)}
          >
            {visible ? (
              <EyeOff className="size-4" aria-hidden="true" />
            ) : (
              <Eye className="size-4" aria-hidden="true" />
            )}
          </button>
        ) : (
          trailing
        )}
      </div>
      {error ? (
        <p id={descriptionId} className="text-sm text-red-700">
          {error}
        </p>
      ) : hint ? (
        <p id={descriptionId} className="text-xs leading-5 text-slate-500">
          {hint}
        </p>
      ) : null}
    </div>
  );
});
