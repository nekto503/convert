import { useState } from 'react';
import { Play, Search, FileSearch } from 'lucide-react';
import { api, tauriAvailable } from '../lib/api';
import { useStore } from '../lib/store';
import { DropZone, FileList, Page, ErrorBox, StudioShell, useStudio, RunBar } from '../components/ui';

export function AudioStudio() {
  const s = useStudio();
  const [format, setFormat] = useState('mp3');
  const [bitrate, setBitrate] = useState(192);
  const [normalize, setNormalize] = useState(false);

  const run = async () => {
    s.setBusy(true); s.setError('');
    try {
      for (const f of s.files) {
        const job = {
          source: f.path,
          format: { container: format },
          bitrate_kbps: bitrate,
          normalize,
          overwrite: 'Rename',
        };
        await api.enqueue({ AudioJob: { payload_json: JSON.stringify(job) } });
      }
      await s.refreshQueue();
      s.toast('success', 'Аудио-задачи добавлены');
    } catch (e) { s.setError(String(e)); }
    s.setBusy(false);
  };

  return (
    <StudioShell title="Audio Studio" subtitle="Конвертация и обработка аудио через FFmpeg">
      <div className="card p-4 grid sm:grid-cols-3 gap-3">
        <label className="text-sm space-y-1">
          <span className="text-slate-400">Формат</span>
          <select className="input" value={format} onChange={(e) => setFormat(e.target.value)}>
            {['mp3', 'wav', 'flac', 'ogg', 'opus', 'm4a'].map((f) => <option key={f}>{f}</option>)}
          </select>
        </label>
        <label className="text-sm space-y-1">
          <span className="text-slate-400">Битрейт: {bitrate} кбит/с</span>
          <input type="range" min={64} max={320} step={32} value={bitrate} onChange={(e) => setBitrate(+e.target.value)} className="w-full" />
        </label>
        <label className="text-sm space-y-1 flex items-end gap-2 pb-2">
          <input type="checkbox" checked={normalize} onChange={(e) => setNormalize(e.target.checked)} />
          <span className="text-slate-400">Нормализация громкости</span>
        </label>
      </div>
      <ErrorBox text={s.error} />
      <RunBar onRun={run} disabled={!s.canRun} />
    </StudioShell>
  );
}

