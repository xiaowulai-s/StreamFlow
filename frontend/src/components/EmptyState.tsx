// 空状态：清晰原因（产品方案 5.3 空状态要求）
import { SignalIcon } from './icons';

interface Props {
  title: string;
  description?: string;
  action?: { label: string; onClick: () => void };
}

export function EmptyState({ title, description, action }: Props) {
  return (
    <div className="flex flex-col items-center justify-center rounded-card border border-dashed border-line bg-surface py-16">
      <div className="flex h-14 w-14 items-center justify-center rounded-full bg-canvas text-slate">
        <SignalIcon size={26} />
      </div>
      <h3 className="mt-4 text-sm font-semibold text-ink">{title}</h3>
      {description && <p className="mt-1 max-w-md text-center text-xs text-slate">{description}</p>}
      {action && (
        <button
          onClick={action.onClick}
          className="mt-4 rounded-lg bg-stream px-4 py-1.5 text-xs font-medium text-white transition-opacity hover:opacity-90"
        >
          {action.label}
        </button>
      )}
    </div>
  );
}
