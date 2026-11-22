import { useState } from 'react';
import { Play, Search, FileSearch } from 'lucide-react';
import { api, tauriAvailable } from '../lib/api';
import { useStore } from '../lib/store';
import { DropZone, FileList, Page, ErrorBox, StudioShell, useStudio, RunBar } from '../components/ui';

export function PdfStudio() {
  const s = useStudio();
  const [op, setOp] = useState('merge');
  const [text, setText] = useState('CONVERT');

  const run = async () => {
    s.setBusy(true); s.setError('');
    try {
      if (op === 'merge') {
        const output = s.files[0].path.replace(/\.[^.]+$/, '_merged.pdf');
        await api.enqueue({ Generic: { operation: 'pdf_merge', payload_json: JSON.stringify({ files: s.files.map((f) => f.path), output }) } });
      } else if (op === 'watermark') {
        for (const f of s.files) {
          const output = f.path.replace(/\.pdf$/i, '_wm.pdf');
          await api.enqueue({ Generic: { operation: 'pdf_watermark', payload_json: JSON.stringify({ source: f.path, output, text }) } });
        }
      } else if (op === 'numbers') {
        for (const f of s.files) {
          const output = f.path.replace(/\.pdf$/i, '_num.pdf');
          await api.enqueue({ Generic: { operation: 'pdf_numbers', payload_json: JSON.stringify({ source: f.path, output }) } });
        }
      } else if (op === 'split') {
        for (const f of s.files) {
          const dir = f.path.replace(/\.pdf$/i, '_pages');
          await api.enqueue({ Generic: { operation: 'pdf_split', payload_json: JSON.stringify({ source: f.path, output_dir: dir }) } });
        }
      }
      await s.refreshQueue();
      s.toast('success', 'PDF-задачи добавлены');
    } catch (e) { s.setError(String(e)); }
    s.setBusy(false);
  };

  return (
    <StudioShell title="PDF Studio" subtitle="Слияние, разделение, водяные знаки, нумерация">
      <div className="card p-4 grid sm:grid-cols-2 gap-3">
        <label className="text-sm space-y-1">
          <span className="text-slate-400">Операция</span>
          <select className="input" value={op} onChange={(e) => setOp(e.target.value)}>
            <option value="merge">Объединить в один PDF</option>
            <option value="split">Разделить по страницам</option>
            <option value="watermark">Добавить водяной знак</option>
            <option value="numbers">Пронумеровать страницы</option>
          </select>
        </label>
        {op === 'watermark' && (
          <label className="text-sm space-y-1">
            <span className="text-slate-400">Текст знака</span>
            <input className="input" value={text} onChange={(e) => setText(e.target.value)} />
          </label>
        )}
      </div>
      <ErrorBox text={s.error} />
      <RunBar onRun={run} disabled={!s.canRun || (op === 'merge' && s.files.length < 2)}
        label={op === 'merge' && s.files.length < 2 ? 'Нужно 2+ файла' : 'Выполнить'} />
    </StudioShell>
  );
}

