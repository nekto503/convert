import { useState } from 'react';
import { Play, Search, FileSearch } from 'lucide-react';
import { api, tauriAvailable } from '../lib/api';
import { useStore } from '../lib/store';
import { DropZone, FileList, Page, ErrorBox, StudioShell, useStudio, RunBar } from '../components/ui';

export function OcrStudio() {
  const s = useStudio();
  const [langs, setLangs] = useState('eng');
  const [result, setResult] = useState<{ text: string; confidence?: number | null } | null>(null);
  const [scanning, setScanning] = useState(false);

  const run = async () => {
    if (!tauriAvailable() || s.files.length === 0) return;
    setScanning(true); s.setError(''); setResult(null);
    try {
      const out = await api.ocrImage(s.files[0].path, langs);
      setResult(out);
    } catch (e) { s.setError(String(e)); }
    setScanning(false);
  };

  return (
    <StudioShell title="OCR Studio" subtitle="Распознавание текста через Tesseract (полностью локально)">
      <div className="card p-4">
        <label className="text-sm space-y-1 block max-w-xs">
          <span className="text-slate-400">Языки</span>
          <select className="input" value={langs} onChange={(e) => setLangs(e.target.value)}>
            <option value="eng">Английский</option>
            <option value="rus">Русский (если установлен)</option>
            <option value="rus+eng">Русский + английский</option>
          </select>
        </label>
      </div>
      <ErrorBox text={s.error} />
      <button className="btn-primary" onClick={run} disabled={!tauriAvailable() || s.files.length === 0 || scanning}>
        <span className="flex items-center gap-2"><FileSearch size={15} /> {scanning ? 'Распознаю...' : 'Распознать первый файл'}</span>
      </button>
      {result && (
        <div className="card p-4">
          <div className="text-xs text-slate-400 mb-2">
            Уверенность: {result.confidence ? result.confidence.toFixed(0) : '—'}%
          </div>
          <pre className="text-sm whitespace-pre-wrap max-h-96 overflow-y-auto">{result.text}</pre>
        </div>
      )}
    </StudioShell>
  );
}

