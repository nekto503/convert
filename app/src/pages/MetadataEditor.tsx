// Metadata Editor (ТЗ §4.10): просмотр/правка EXIF, палитра, удаление метаданных.
import { useState } from 'react';
import { Info, Palette, Trash2 } from 'lucide-react';
import { api, tauriAvailable } from '../lib/api';
import { useStore } from '../lib/store';

interface ExifField { key: string; label: string; value: string }
interface MetaInfo {
  width: number; height: number; format: string; file_size: number;
  exif: ExifField[];
}

export default function MetadataEditor({ files }: { files: { path: string; name: string }[] }) {
  const { toast } = useStore();
  const [meta, setMeta] = useState<MetaInfo | null>(null);
  const [palette, setPalette] = useState<{ hex: string; share: number }[] | null>(null);
  const [busy, setBusy] = useState(false);

  const file = files[0];
  const loadMeta = async () => {
    if (!file || !tauriAvailable()) return;
    setBusy(true);
    try {
      const m = (await api.imageMetadata(file.path)) as unknown as MetaInfo;
      setMeta(m);
      setPalette(await api.imagePalette(file.path));
    } catch { setMeta(null); }
    setBusy(false);
  };

  const strip = async () => {
    if (!file || !tauriAvailable()) return;
    if (!confirm('Удалить все личные метаданные (GPS, камера, автор)? Действие нельзя отменить.')) return;
    try {
      await window.__TAURI_INTERNALS__!.invoke('image_strip_metadata', { path: file.path });
      toast('success', 'Метаданные удалены');
      loadMeta();
    } catch (e) { toast('error', String(e)); }
  };

  if (!file) return null;

  return (
    <div className="card p-4 space-y-3">
      <div className="flex items-center gap-3">
        <div className="font-medium text-sm flex-1">Метаданные: {file.name}</div>
        <button className="btn-ghost text-xs" onClick={loadMeta} disabled={busy}>
          <span className="flex items-center gap-1"><Info size={13} /> {meta ? 'Обновить' : 'Показать'}</span>
        </button>
      </div>

      {meta && (
        <>
          <div className="text-sm grid sm:grid-cols-3 gap-2">
            <div><span className="text-slate-400 text-xs">Формат</span><div>{meta.format}</div></div>
            <div><span className="text-slate-400 text-xs">Разрешение</span><div>{meta.width}×{meta.height}</div></div>
            <div><span className="text-slate-400 text-xs">Вес</span><div>{(meta.file_size / 1024).toFixed(1)} КБ</div></div>
          </div>
          {meta.exif.length > 0 && (
            <div className="text-xs max-h-40 overflow-y-auto divide-y divide-slate-100 dark:divide-slate-800">
              {meta.exif.map((f) => (
                <div key={f.key} className="flex justify-between py-1">
                  <span className="text-slate-400">{f.label || f.key}</span>
                  <span className="ml-3 truncate max-w-[60%]">{f.value}</span>
                </div>
              ))}
            </div>
          )}
          <button className="btn-ghost text-xs text-red-400" onClick={strip}>
            <span className="flex items-center gap-1"><Trash2 size={13} /> Удалить личные метаданные</span>
          </button>
        </>
      )}

      {palette && palette.length > 0 && (
        <div>
          <div className="text-xs text-slate-400 mb-1 flex items-center gap-1"><Palette size={12} /> Палитра</div>
          <div className="flex gap-1.5">
            {palette.map((c) => (
              <div key={c.hex} className="text-center">
                <div className="w-9 h-9 rounded-lg border border-slate-200/50" style={{ background: c.hex }} title={c.hex} />
                <div className="text-[10px] text-slate-400 mt-0.5">{(c.share * 100).toFixed(0)}%</div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
