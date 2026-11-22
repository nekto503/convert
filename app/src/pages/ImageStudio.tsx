import { useState } from 'react';
import { Play, Search, FileSearch } from 'lucide-react';
import { api, tauriAvailable } from '../lib/api';
import { useStore } from '../lib/store';
import { DropZone, FileList, Page, ErrorBox, StudioShell, useStudio, RunBar } from '../components/ui';
import MetadataEditor from './MetadataEditor';

export function ImageStudio() {
  const s = useStudio();
  const [format, setFormat] = useState('Png');
  const [quality, setQuality] = useState(90);
  const [width, setWidth] = useState('');
  const [height, setHeight] = useState('');

  const run = async () => {
    s.setBusy(true); s.setError("");
    try {
      for (const f of s.files) {
        const job: Record<string, unknown> = {
          source: f.path,
          target_format: { [format.toLowerCase()]: format === 'WebP' ? { lossless: false } : null },
          encode: { quality, dpi: null },
          overwrite: 'Rename',
        };
        const resize: Record<string, unknown> = {};
        if (width) resize.width = Number(width);
        if (height) resize.height = Number(height);
        if (Object.keys(resize).length) {
          resize.mode = 'Fit'; resize.high_quality = true;
          job.resize = resize;
        }
        await api.enqueue({ ImageJob: { payload_json: JSON.stringify(job) } });
      }
      await s.refreshQueue();
      s.toast('success', `Задач в очереди: ${s.files.length}`);
    } catch (e) { s.setError(String(e)); }
    s.setBusy(false);
  };

  return (
    <StudioShell title="Image Studio" subtitle="Конвертация, сжатие, масштабирование изображений">
      <div className="card p-4 grid sm:grid-cols-2 lg:grid-cols-4 gap-3">
        <label className="text-sm space-y-1">
          <span className="text-slate-400">Формат</span>
          <select className="input" value={format} onChange={(e) => setFormat(e.target.value)}>
            {['Png', 'Jpeg', 'WebP', 'Bmp', 'Gif', 'Tiff'].map((f) => <option key={f}>{f}</option>)}
          </select>
        </label>
        <label className="text-sm space-y-1">
          <span className="text-slate-400">Качество: {quality}</span>
          <input type="range" min={10} max={100} value={quality} onChange={(e) => setQuality(+e.target.value)} className="w-full" />
        </label>
        <label className="text-sm space-y-1">
          <span className="text-slate-400">Ширина (px)</span>
          <input className="input" value={width} onChange={(e) => setWidth(e.target.value)} placeholder="как есть" />
        </label>
        <label className="text-sm space-y-1">
          <span className="text-slate-400">Высота (px)</span>
          <input className="input" value={height} onChange={(e) => setHeight(e.target.value)} placeholder="как есть" />
        </label>
      </div>
      <MetadataEditor files={s.files} />
      <ErrorBox text={s.error} />
      <RunBar onRun={run} disabled={!s.canRun} label={`Конвертировать (${s.files.length})`} />
    </StudioShell>
  );
}

