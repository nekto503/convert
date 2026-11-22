import { useState } from 'react';
import { Play, Search, FileSearch } from 'lucide-react';
import { api, tauriAvailable } from '../lib/api';
import { useStore } from '../lib/store';
import { DropZone, FileList, Page, ErrorBox, StudioShell, useStudio, RunBar } from '../components/ui';

export function DocumentStudio() {
  const s = useStudio();
  const [target, setTarget] = useState('pdf');

  const run = async () => {
    s.setBusy(true); s.setError('');
    try {
      for (const f of s.files) {
        await api.enqueue({ OfficeConvert: { source: f.path, target, output_dir: null } });
      }
      await s.refreshQueue();
      s.toast('success', 'Задачи конвертации документов добавлены');
    } catch (e) { s.setError(String(e)); }
    s.setBusy(false);
  };

  return (
    <StudioShell title="Document Studio" subtitle="Конвертация офисных документов через LibreOffice (локально)">
      <div className="card p-4">
        <label className="text-sm space-y-1 block max-w-xs">
          <span className="text-slate-400">Целевой формат</span>
          <select className="input" value={target} onChange={(e) => setTarget(e.target.value)}>
            {['pdf', 'docx', 'odt', 'rtf', 'txt', 'xlsx', 'ods', 'csv', 'pptx', 'odp'].map((f) => <option key={f}>{f.toUpperCase()}</option>)}
          </select>
        </label>
      </div>
      <ErrorBox text={s.error} />
      <RunBar onRun={run} disabled={!s.canRun} />
    </StudioShell>
  );
}

