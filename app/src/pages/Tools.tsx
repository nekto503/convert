import { useState } from 'react';
import { Search, QrCode, Copy, Zap, FolderSearch } from 'lucide-react';
import { api, tauriAvailable, type ScanInfo } from '../lib/api';
import { useStore, formatBytes } from '../lib/store';
import { DropZone, FileList, Page, ErrorBox } from '../components/ui';

export default function Tools() {
  const { files, toast } = useStore();
  const [error, setError] = useState('');
  const [dupGroups, setDupGroups] = useState<{ hash: string; files: string[]; size: number }[] | null>(null);
  const [smart, setSmart] = useState<{ variants: { quality: number; estimated_size: number; saving_percent: number }[]; recommended: number; note: string } | null>(null);
  const [scanInfo, setScanInfo] = useState<ScanInfo | null>(null);
  const [busy, setBusy] = useState('');

  const guard = (name: string) => {
    if (!tauriAvailable()) { setError('Инструмент доступен в приложении CONVERT'); return false; }
    if (files.length === 0) { setError('Сначала добавьте файл'); return false; }
    setBusy(name); setError('');
    return true;
  };

  const dupScan = async () => {
    if (!guard('dup')) return;
    try {
      // Дубликаты ищутся в папке первого файла.
      const path = files[0].path.replace(/[^/\\]+$/, '');
      setDupGroups(await api.findDuplicates(path));
      toast('info', 'Поиск завершён');
    } catch (e) { setError(String(e)); }
    setBusy('');
  };

  const smartAnalyze = async () => {
    if (!guard('smart')) return;
    try { setSmart(await api.smartCompressAnalyze(files[0].path, 'balanced')); }
    catch (e) { setError(String(e)); }
    setBusy('');
  };

  const scanOne = async () => {
    if (!guard('scan')) return;
    try { setScanInfo(await api.scanFile(files[0].path)); }
    catch (e) { setError(String(e)); }
    setBusy('');
  };

  return (
    <Page title="Инструменты" subtitle="Format Detector, Duplicate Finder, Smart Compress, QR, кодировки">
      <DropZone />
      <FileList />
      <div className="grid sm:grid-cols-3 gap-3">
        <button className="btn-ghost flex items-center justify-center gap-2 py-3" onClick={scanOne} disabled={!!busy}>
          <Search size={16} /> Format Detector
        </button>
        <button className="btn-ghost flex items-center justify-center gap-2 py-3" onClick={dupScan} disabled={!!busy}>
          <FolderSearch size={16} /> Дубликаты
        </button>
        <button className="btn-ghost flex items-center justify-center gap-2 py-3" onClick={smartAnalyze} disabled={!!busy}>
          <Zap size={16} /> Smart Compress
        </button>
      </div>
      <ErrorBox text={error} />

      {scanInfo && (
        <div className="card p-4 text-sm space-y-1">
          <div className="font-medium">{String(scanInfo.file_name)}</div>
          <div>Формат: <b>{String(scanInfo.detected_format)}</b> ({String(scanInfo.category)})</div>
          <div>Размер: {formatBytes(Number(scanInfo.size_bytes))}</div>
          {(scanInfo.details as [string, string][] | undefined)?.map(([k, v]) => (
            <div key={k}>{k}: {v}</div>
          ))}
        </div>
      )}

      {smart && (
        <div className="card p-4">
          <div className="text-sm text-slate-400">{smart.note}</div>
          <div className="mt-2 space-y-1">
            {smart.variants.map((v, i) => (
              <div key={v.quality} className={`text-sm flex justify-between px-3 py-2 rounded-xl ${i === smart.recommended ? 'bg-accent/10 text-accent' : ''}`}>
                <span>Качество {v.quality}</span>
                <span>{formatBytes(v.estimated_size)} (−{v.saving_percent.toFixed(0)}%)</span>
              </div>
            ))}
          </div>
        </div>
      )}

      {dupGroups && (
        <div className="card p-4 space-y-3 max-h-96 overflow-y-auto">
          {dupGroups.length === 0 && <div className="text-sm text-slate-400">Дубликаты не найдены</div>}
          {dupGroups.map((g) => (
            <div key={g.hash} className="border border-slate-200/60 dark:border-slate-700/60 rounded-xl p-3">
              <div className="text-xs text-slate-400 mb-1">{g.files.length} копий × {formatBytes(g.size)}</div>
              {g.files.map((f) => <div key={f} className="text-sm truncate">{f}</div>)}
            </div>
          ))}
        </div>
      )}
    </Page>
  );
}
