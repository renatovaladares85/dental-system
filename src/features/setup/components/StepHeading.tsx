interface StepHeadingProps {
  eyebrow: string;
  title: string;
  description: string;
}

export function StepHeading({ eyebrow, title, description }: StepHeadingProps) {
  return (
    <header className="mb-7">
      <p className="mb-2 text-xs font-bold uppercase tracking-[0.16em] text-petrol-700">
        {eyebrow}
      </p>
      <h1 className="text-3xl font-bold tracking-tight text-slate-950 sm:text-4xl">
        {title}
      </h1>
      <p className="mt-3 max-w-2xl text-base leading-7 text-slate-600">{description}</p>
    </header>
  );
}
