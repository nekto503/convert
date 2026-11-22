import { useEffect, useState } from 'react';
import { Trash2, RefreshCw } from 'lucide-react';
import { api, tauriAvailable, type HistoryItem } from '../lib/api';
import { useStore } from '../lib/store';
import { Page } from '../components/ui';

export default function HistoryPage() {
  const { toast } = useStore();
  const [items, setItems] = useState<HistoryItem[]>([]);
  const [query, setQuery] = useState('');
  const [status, setStatus] = useState('');

  const load = async () => {
    if (!tauriAvailable()) return;
    try { setItems(await api.history(query || undefined, status || undefined)); }
    catch { /* ignore */ }
  };

  useEffect(() => { load(); }, [query, status]); // eslint-disable-line

  return (
    <Page title="История операций" subtitle="Все операции CONVERT с возможностью поиска и фильтрации">
      <div className="flex gap-3">
        <input className="input" placeholder="Поиск по операциям и файлам..." value={query} onChange={(e) => setQuery(e.target.value)} />
        <select className="input max-w-40" value={status} onChange={(e) => setStatus(e.target.value)}>
          <option value="">Все статусы</option>
          <option value="success">Успех</option>
          <option value="error">Ошибка</option>
          <option value="cancelled">Отменено</option>
        </select>
        <button className="btn-ghost" onClick={load}><RefreshCw size={15} /></button>
        <button className="btn-ghost text-red-400" onClick={async () => {
          if (!tauriAvailable()) return;
          await api.historyClear();
          setItems([]);
          toast('info', 'История очищена');
        }}><Trash2 size={15} /></button>
      </div>
      <div className="card divide-y divide-slate-100 dark:divide-slate-800">
        {items.length === 0 && <div className="p-4 text-sm text-slate-400">Пока нет операций</div>}
        {items.map((h) => (
          <div key={h.id} className="p-4 text-sm flex items-center gap-3">
            <span className={`badge ${h.status === 'success' ? 'bg-emerald-400/15 text-emerald-500' : h.status === 'error' ? 'bg-red-400/15 text-red-400' : 'bg-slate-400/15 text-slate-400'}`}>
              {h.status === 'success' ? 'успех' : h.status === 'error' ? 'ошибка' : 'отменено'}
            </span>
            <div className="flex-1 min-w-0">
              <div className="font-medium">{h.operation}</div>
              <div className="text-xs text-slate-400 truncate">{h.source}{h.output ? ` → ${h.output}` : ''}</div>
              {h.error && <div className="text-xs text-red-400">{h.error}</div>}
            </div>
            <div className="text-xs text-slate-400 whitespace-nowrap">
              {(h.duration_ms / 1000).toFixed(1)} с
              <br />
              {new Date(h.timestamp * 1000).toLocaleString('ru')}
            </div>
            {h.output && (
              <button className="btn-ghost text-xs" onClick={() => tauriAvailable() && api.showInFolder(h.output!)}>
                Открыть папку
              </button>
            )}
          </div>
        ))}
      </div>
    </Page>
  );
}
