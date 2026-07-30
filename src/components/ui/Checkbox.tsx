import type { InputHTMLAttributes } from 'react';
import { forwardRef, useId } from 'react';

export interface CheckboxProps extends Omit<
  InputHTMLAttributes<HTMLInputElement>,
  'type'
> {
  label: string;
  description?: string | undefined;
  error?: string | undefined;
}

export const Checkbox = forwardRef<HTMLInputElement, CheckboxProps>(function Checkbox(
  { label, description, error, id, ...props },
  ref,
) {
  const generatedId = useId();
  const fieldId = id ?? generatedId;
  const descriptionId = `${fieldId}-description`;

  return (
    <div>
      <label
        htmlFor={fieldId}
        className="flex cursor-pointer items-start gap-3 rounded-lg border border-slate-200 bg-white p-3.5 hover:border-petrol-300"
      >
        <input
          ref={ref}
          id={fieldId}
          type="checkbox"
          className="mt-0.5 size-4 shrink-0 accent-petrol-700 focus-visible:outline-3 focus-visible:outline-offset-2 focus-visible:outline-petrol-500"
          aria-invalid={Boolean(error)}
          aria-describedby={description || error ? descriptionId : undefined}
          {...props}
        />
        <span>
          <span className="block text-sm font-semibold text-slate-800">{label}</span>
          {description ? (
            <span className="mt-1 block text-xs leading-5 text-slate-500">
              {description}
            </span>
          ) : null}
        </span>
      </label>
      {error ? (
        <p id={descriptionId} className="mt-1 text-sm text-red-700">
          {error}
        </p>
      ) : null}
    </div>
  );
});
