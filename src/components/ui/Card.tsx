import type { HTMLAttributes } from 'react';

import { cn } from './cn';

export function Card({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn(
        'rounded-xl border border-slate-200 bg-white shadow-[0_18px_50px_-32px_rgba(13,37,53,0.45)]',
        className,
      )}
      {...props}
    />
  );
}
