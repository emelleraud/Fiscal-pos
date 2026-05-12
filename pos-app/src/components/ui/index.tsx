/**
 * Composants UI primitifs.
 * Thème sombre QSR : fort contraste, grandes zones tactiles (min 48x48px).
 * Tailwind CSS uniquement — pas de bibliothèque externe.
 */

import React from 'react';

// ---------------------------------------------------------------------------
// Button
// ---------------------------------------------------------------------------

type ButtonVariant = 'primary' | 'secondary' | 'danger' | 'ghost' | 'success';
type ButtonSize = 'sm' | 'md' | 'lg' | 'xl';

interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  loading?: boolean;
  fullWidth?: boolean;
  children: React.ReactNode;
}

const variantClasses: Record<ButtonVariant, string> = {
  primary:   'bg-blue-600 hover:bg-blue-500 active:bg-blue-700 text-white border-transparent',
  secondary: 'bg-gray-700 hover:bg-gray-600 active:bg-gray-800 text-white border-gray-600',
  danger:    'bg-red-700 hover:bg-red-600 active:bg-red-800 text-white border-transparent',
  ghost:     'bg-transparent hover:bg-gray-800 active:bg-gray-900 text-gray-300 border-gray-700',
  success:   'bg-green-700 hover:bg-green-600 active:bg-green-800 text-white border-transparent',
};

const sizeClasses: Record<ButtonSize, string> = {
  sm:  'px-3 py-2 text-sm min-h-[36px]',
  md:  'px-4 py-3 text-base min-h-[44px]',
  lg:  'px-6 py-4 text-lg min-h-[56px]',
  xl:  'px-8 py-5 text-xl min-h-[64px]',
};

export function Button({
  variant = 'primary',
  size = 'md',
  loading = false,
  fullWidth = false,
  disabled,
  children,
  className = '',
  ...props
}: ButtonProps): React.ReactElement {
  const isDisabled = disabled || loading;
  return (
    <button
      {...props}
      disabled={isDisabled}
      className={[
        'inline-flex items-center justify-center gap-2',
        'font-semibold rounded-xl border',
        'transition-colors duration-100',
        'focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 focus:ring-offset-gray-900',
        variantClasses[variant],
        sizeClasses[size],
        fullWidth ? 'w-full' : '',
        isDisabled ? 'opacity-50 cursor-not-allowed pointer-events-none' : 'cursor-pointer',
        className,
      ].join(' ')}
    >
      {loading && (
        <span className="animate-spin h-4 w-4 border-2 border-current border-t-transparent rounded-full" />
      )}
      {children}
    </button>
  );
}

// ---------------------------------------------------------------------------
// Badge
// ---------------------------------------------------------------------------

type BadgeVariant = 'default' | 'success' | 'warning' | 'danger' | 'info';

interface BadgeProps {
  variant?: BadgeVariant;
  children: React.ReactNode;
  className?: string;
}

const badgeVariants: Record<BadgeVariant, string> = {
  default: 'bg-gray-700 text-gray-200',
  success: 'bg-green-900 text-green-300',
  warning: 'bg-yellow-900 text-yellow-300',
  danger:  'bg-red-900 text-red-300',
  info:    'bg-blue-900 text-blue-300',
};

export function Badge({ variant = 'default', children, className = '' }: BadgeProps): React.ReactElement {
  return (
    <span
      className={[
        'inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium',
        badgeVariants[variant],
        className,
      ].join(' ')}
    >
      {children}
    </span>
  );
}

// ---------------------------------------------------------------------------
// Card
// ---------------------------------------------------------------------------

interface CardProps {
  children: React.ReactNode;
  className?: string;
  onClick?: () => void;
}

export function Card({ children, className = '', onClick }: CardProps): React.ReactElement {
  return (
    <div
      onClick={onClick}
      className={[
        'bg-gray-800 border border-gray-700 rounded-2xl',
        onClick ? 'cursor-pointer hover:bg-gray-750 hover:border-gray-600 active:bg-gray-700 transition-colors' : '',
        className,
      ].join(' ')}
    >
      {children}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Spinner
// ---------------------------------------------------------------------------

export function Spinner({ size = 'md' }: { size?: 'sm' | 'md' | 'lg' }): React.ReactElement {
  const sizes = { sm: 'h-4 w-4', md: 'h-8 w-8', lg: 'h-12 w-12' };
  return (
    <div
      className={[
        sizes[size],
        'animate-spin rounded-full border-4 border-gray-700 border-t-blue-500',
      ].join(' ')}
    />
  );
}

// ---------------------------------------------------------------------------
// AlertBanner
// ---------------------------------------------------------------------------

interface AlertBannerProps {
  message: string;
  type?: 'error' | 'warning' | 'info';
  onDismiss?: () => void;
}

export function AlertBanner({ message, type = 'error', onDismiss }: AlertBannerProps): React.ReactElement {
  const styles = {
    error:   'bg-red-900/80 border-red-700 text-red-200',
    warning: 'bg-yellow-900/80 border-yellow-700 text-yellow-200',
    info:    'bg-blue-900/80 border-blue-700 text-blue-200',
  };
  return (
    <div className={`flex items-center gap-3 px-4 py-3 border rounded-xl text-sm ${styles[type]}`}>
      <span className="flex-1">{message}</span>
      {onDismiss && (
        <button
          onClick={onDismiss}
          className="shrink-0 text-current opacity-70 hover:opacity-100 text-lg leading-none"
          aria-label="Fermer"
        >
          ✕
        </button>
      )}
    </div>
  );
}
