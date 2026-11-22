import { useState } from 'react';
import { Play, Search, FileSearch } from 'lucide-react';
import { api, tauriAvailable } from '../lib/api';
import { useStore } from '../lib/store';
import { DropZone, FileList, Page, ErrorBox, StudioShell, useStudio, RunBar } from '../components/ui';

export function VideoStudio() {
  const s = useStudio();
  const [container, setContainer] = useState('mp4');
  const [crf, setCrf] = useState(23);
  const [preset, setPreset] = useState('medium');

  const run = async () => {
    s.setBusy(true); s.setError('');
    try {
      const formats: Record<string, unknown> = {
        mp4: { container: 'mp4', video_codec: 'h264', audio_codec: 'aac' },
        mkv: { container: 'mkv', video_codec: 'hevc', audio_codec: 'opus' },
        webm: { container: 'webm', video_codec: 'vp9', audio_codec: 'opus' },
      };
      for (const f of s.files) {
        const job = {
          source: f.path,
          format: formats[container],
          crf, preset,
          audio_bitrate_kbps: 128,
          overwrite: 'Rename',
        };
        await api.enqueue({ VideoJob: { payload_json: JSON.stringify(job) } });
      }
      await s.refreshQueue();
      s.toast('success', 'Видео-задачи добавлены');
    } catch (e) { s.setError(String(e)); }
    s.setBusy(false);
  };

  return (
    <StudioShell title="Video Studio" subtitle="Конвертация видео через FFmpeg (локально)">
      <div className="card p-4 grid sm:grid-cols-3 gap-3">
        <label className="text-sm space-y-1">
          <span className="text-slate-400">Контейнер</span>
          <select className="input" value={container} onChange={(e) => setContainer(e.target.value)}>
            <option value="mp4">MP4 (H.264)</option>
            <option value="mkv">MKV (HEVC)</option>
            <option value="webm">WebM (VP9)</option>
          </select>
        </label>
        <label className="text-sm space-y-1">
          <span className="text-slate-400">Качество CRF: {crf}</span>
          <input type="range" min={18} max={32} value={crf} onChange={(e) => setCrf(+e.target.value)} className="w-full" />
        </label>
        <label className="text-sm space-y-1">
          <span className="text-slate-400">Скорость кодирования</span>
          <select className="input" value={preset} onChange={(e) => setPreset(e.target.value)}>
            {['ultrafast', 'fast', 'medium', 'slow'].map((p) => <option key={p}>{p}</option>)}
          </select>
        </label>
      </div>
      <ErrorBox text={s.error} />
      <RunBar onRun={run} disabled={!s.canRun} />
    </StudioShell>
  );
}

