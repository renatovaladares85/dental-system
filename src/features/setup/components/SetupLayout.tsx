import { Check, LockKeyhole } from 'lucide-react';
import type { ReactNode } from 'react';

import { AppLogo } from '../../../components/brand/AppLogo';
import { cn } from '../../../components/ui/cn';
import type { SecurityDiagnostics } from '../types';

const SETUP_STEPS = [
  { number: 1, label: 'Início' },
  { number: 2, label: 'Clínica' },
  { number: 3, label: 'Administrador' },
  { number: 4, label: 'Segurança' },
  { number: 5, label: 'Confirmação' },
] as const;

const PROGRESS_WIDTHS = ['w-1/5', 'w-2/5', 'w-3/5', 'w-4/5', 'w-full'] as const;

interface SetupLayoutProps {
  currentStep: number;
  diagnostics: SecurityDiagnostics;
  children: ReactNode;
}

export function SetupLayout({ currentStep, diagnostics, children }: SetupLayoutProps) {
  return (
    <div className="min-h-screen bg-slate-50 text-slate-950">
      <a
        href="#main-content"
        className="fixed left-4 top-3 z-50 -translate-y-20 rounded-lg bg-petrol-950 px-4 py-2 text-sm font-semibold text-white focus:translate-y-0"
      >
        Ir para o conteúdo
      </a>
      <div className="grid min-h-screen lg:grid-cols-[19rem_1fr]">
        <aside className="hidden flex-col bg-petrol-950 px-8 py-8 text-white lg:flex">
          <AppLogo inverted />
          <nav className="mt-16" aria-label="Etapas da configuração">
            <ol className="grid gap-2">
              {SETUP_STEPS.map((step) => {
                const complete = currentStep > step.number;
                const current = currentStep === step.number;

                return (
                  <li key={step.number}>
                    <div
                      className={cn(
                        'flex items-center gap-3 rounded-lg px-3 py-3 text-sm',
                        current && 'bg-white/10 font-semibold',
                        complete ? 'text-petrol-100' : 'text-slate-300',
                      )}
                      aria-current={current ? 'step' : undefined}
                    >
                      <span
                        className={cn(
                          'grid size-8 shrink-0 place-items-center rounded-full border text-xs font-bold',
                          current && 'border-petrol-300 bg-petrol-600 text-white',
                          complete && 'border-emerald-400 bg-emerald-500 text-white',
                          !current && !complete && 'border-slate-600',
                        )}
                        aria-hidden="true"
                      >
                        {complete ? <Check className="size-4" /> : step.number}
                      </span>
                      {step.label}
                    </div>
                  </li>
                );
              })}
            </ol>
          </nav>
          <div className="mt-auto flex gap-3 border-t border-white/10 pt-6 text-xs leading-5 text-slate-300">
            <LockKeyhole
              className="mt-0.5 size-4 shrink-0 text-petrol-300"
              aria-hidden="true"
            />
            <p>Seus dados permanecem neste computador e são protegidos localmente.</p>
          </div>
        </aside>

        <main id="main-content" className="flex min-w-0 flex-col">
          <header className="border-b border-slate-200 bg-white px-5 py-4 lg:hidden">
            <div className="flex items-center justify-between gap-4">
              <AppLogo compact />
              <p className="text-sm font-semibold text-slate-700">
                Etapa {currentStep} de {SETUP_STEPS.length}
              </p>
            </div>
            <div
              className="mt-3 h-1.5 overflow-hidden rounded-full bg-slate-200"
              role="progressbar"
              aria-label="Progresso da configuração"
              aria-valuemin={1}
              aria-valuemax={SETUP_STEPS.length}
              aria-valuenow={currentStep}
            >
              <div
                className={cn(
                  'h-full rounded-full bg-petrol-600 transition-[width]',
                  PROGRESS_WIDTHS[currentStep - 1] ?? 'w-full',
                )}
              />
            </div>
          </header>
          {!diagnostics.distributionReady ? (
            <div className="border-b border-amber-200 bg-amber-50 px-6 py-2 text-center text-xs font-medium text-amber-950">
              Build de desenvolvimento — SQLCipher{' '}
              {diagnostics.sqlcipherVersion ?? 'não detectado'}; distribuição exige{' '}
              {diagnostics.minimumDistributionVersion} ou superior.
            </div>
          ) : null}
          <div className="flex flex-1 items-center justify-center px-5 py-8 sm:px-8 lg:px-12">
            <div className="w-full max-w-4xl">{children}</div>
          </div>
        </main>
      </div>
    </div>
  );
}
