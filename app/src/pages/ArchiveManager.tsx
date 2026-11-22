import { useState } from 'react';
import { Play, Search, FileSearch } from 'lucide-react';
import { api, tauriAvailable } from '../lib/api';
import { useStore } from '../lib/store';
import { DropZone, FileList, Page, ErrorBox, StudioShell, useStudio, RunBar } from '../components/ui';

export function ArchiveManager() {
  const s = useStudio();
  const [format, setFormat] = useState('zip');
  const [password, setPassword] = useState('');
  const [listing, setListing] = useState<{ name: string; size: number; is_dir: boolean }[] | null>(null);

  const create = async () => {
    s.setBusy(true); s.setError('');
    try {
      const output = s.files[0].path.replace(/\.[^.]+$/, `_archive.${format}`);
      await api.enqueue({
        Generic: {
          operation: 'archive_create',
          payload_json: JSON.stringify({ items: s.files.map((f) => f.path), output, format, password: password || null }),
        },
      });
      await s.refreshQueue();
      s.toast('success', 'Задача архивации добавлена');
    } catch (e) { s.setError(String(e)); }
    s.setBusy(false);
  };

  const extract = async () => {
    s.setBusy(true); s.setError('');
    try {
      const dir = s.files[0].path.replace(/\.[^.]+$/, '_unpacked');
      await api.enqueue({
        Generic: {
          operation: 'archive_extract',
          payload_json: JSON.stringify({ archive: s.files[0].path, output_dir: dir, password: password || null }),
        },
      });
      await s.refreshQueue();
      s.toast('success', 'Задача распаковки добавлена');
    } catch (e) { s.setError(String(e)); }
    s.setBusy(false);
  };

  const list = async () => {
    if (!tauriAvailable() || s.files.length === 0) return;
    try { setListing(await api.archiveList(s.files[0].path)); }
    catch (e) { s.setError(String(e)); }
  };

  return (
    <StudioShell title="Менеджер архивов" subtitle="ZIP / 7Z / TAR.GZ: создание, распаковка, пароли, защита от zip-бомб">
      <div className="card p-4 grid sm:grid-cols-3 gap-3">
        <label className="text-sm space-y-1">
          <span className="text-slate-400">Формат</span>
          <select className="input" value={format} onChange={(e) => setFormat(e.target.value)}>
            {['zip', '7z', 'tar', 'tar.gz'].map((f) => <option key={f}>{f}</option>)}
          </select>
        </label>
        <label className="text-sm space-y-1 sm:col-span-2">
          <span className="text-slate-400">Пароль (необязательно)</span>
          <input className="input" type="password" value={password} onChange={(e) => setPassword(e.target.value)} placeholder="без пароля" />
        </label>
      </div>
      <ErrorBox text={s.error} />
      <div className="flex flex-wrap gap-2">
        <RunBar onRun={create} disabled={!s.canRun} label="Создать архив" />
        <button className="btn-ghost" onClick={extract} disabled={!s.canRun}>Распаковать первый</button>
        <button className="btn-ghost" onClick={list} disabled={!tauriAvailable() || s.files.length === 0}>
          <span className="flex items-center gap-2"><Search size={14} /> Содержимое</span>
        </button>
      </div>
      {listing && (
        <div className="card p-4 max-h-80 overflow-y-auto">
          {listing.map((e, i) => (
            <div key={i} className="text-sm py-1 flex justify-between">
              <span className={e.is_dir ? 'text-slate-400' : ''}>{e.name}</span>
              <span className="text-slate-400">{e.is_dir ? 'папка' : `${e.size} Б`}</span>
            </div>
          ))}
        </div>
      )}
    </StudioShell>
  );
}
