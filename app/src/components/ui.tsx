// Общие компоненты UI.

import { ReactNode, useRef } from 'react';
import { UploadCloud, X } from 'lucide-react';
import { useStore, formatBytes, type DroppedFile } from '../lib/store';
import { tauriAvailable } from '../lib/api';

export function DropZone({ hint }: { hint?: string }) {
  const { addFiles, toast } = useStore();
  const inputRef = useRef<HTMLInputElement>(null);

  const handleFiles = (list: FileList | null) => {
    if (!list) return;
    const files: DroppedFile[] = Array.from(list).map((f) => ({
      path: (f as File & { path?: string }).path || f.name,
      name: f.name,
      size: f.size,
    }));
    addFiles(files);
    toast('info', `Добавлено файлов: ${files.length}`);
  };

  return (
    <div
      onDragOver={(e) => e.preventDefault()}
      onDrop={(e) => {
        e.preventDefault();
        handleFiles(e.dataTransfer.files);
      }}
      onClick={() => inputRef.current?.click()}
      className="border-2 border-dashed border-slate-300 dark:border-slate-600 rounded-2xl
                 p-10 text-center cursor-pointer transition-colors hover:border-accent hover:bg-accent/5"
    >
      <UploadCloud size={40} className="mx-auto text-slate-400 mb-3" />
      <div className="font-medium">Перетащите файлы сюда</div>
      <div className="text-sm text-slate-400 mt-1">{hint || 'или нажмите для выбора'}</div>
      <input
        ref={inputRef}
        type="file"
        multiple
        hidden
        onChange={(e) => handleFiles(e.target.files)}
      />
    </div>
  );
}

export function FileList() {
  const { files, removeFile, clearFiles } = useStore();
  if (files.length === 0) return null;
  return (
    <div className="card p-4">
      <div className="flex items-center justify-between mb-2">
        <div className="font-medium text-sm">Файлы ({files.length})</div>
        <button className="text-xs text-slate-400 hover:text-red-400" onClick={clearFiles}>
          Очистить
        </button>
      </div>
      <div className="space-y-1">
        {files.map((f) => (
          <div key={f.path} className="flex items-center gap-2 text-sm px-2 py-1.5 rounded-lg hover:bg-slate-100 dark:hover:bg-slate-800">
            <span className="flex-1 truncate">{f.name}</span>
            <span className="text-slate-400 text-xs">{formatBytes(f.size)}</span>
            <button onClick={() => removeFile(f.path)} className="text-slate-400 hover:text-red-400">
              <X size={14} />
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}

export function Page({ title, subtitle, children }: { title: string; subtitle?: string; children: ReactNode }) {
  return (
    <div className="p-6 max-w-5xl mx-auto space-y-5">
      <div>
        <h1 className="text-2xl font-bold">{title}</h1>
        {subtitle && <p className="text-sm text-slate-400 mt-1">{subtitle}</p>}
      </div>
      {children}
    </div>
  );
}

export function ErrorBox({ text }: { text: string }) {
  if (!text) return null;
  return (
    <div className="card border-red-300/60 p-3 text-sm text-red-500">{text}</div>
  );
}

import { useState } from 'react';
import { Play } from 'lucide-react';

export function StudioShell({ title, subtitle, children }: { title: string; subtitle: string; children: React.ReactNode }) {
  return (
    <div className="p-6 max-w-5xl mx-auto space-y-5">
      <div>
        <h1 className="text-2xl font-bold">{title}</h1>
        <p className="text-sm text-slate-400 mt-1">{subtitle}</p>
      </div>
      <DropZone />
      <FileList />
      {children}
    </div>
  );
}

export function useStudio() {
  const { files, toast, refreshQueue } = useStore();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const canRun = tauriAvailable() && files.length > 0 && !busy;
  return { files, toast, refreshQueue, busy, setBusy, error, setError, canRun };
}

export function RunBar({ onRun, label = 'Запустить', disabled }: { onRun: () => void; label?: string; disabled?: boolean }) {
  return (
    <button className="btn-primary" onClick={onRun} disabled={disabled}>
      <span className="flex items-center gap-2"><Play size={15} /> {label}</span>
    </button>
  );
}
