// AI File Assistant (ТЗ §5.2): локальный помощник на правилах.
// Пишем задачу обычным языком → план + предлагаемые задачи (с подтверждением).
import { useState } from 'react';
import { Sparkles, Send, CheckCircle2, AlertTriangle } from 'lucide-react';
import { api2, tauriAvailable, type AssistantReplyInfo } from '../lib/api';
import { useStore } from '../lib/store';
import { Page, ErrorBox } from '../components/ui';

const EXAMPLES = [
  'Уменьши размер этого видео',
  'Преобразуй все изображения в WEBP',
  'Извлеки звук из этого видео',
  'Объедини эти PDF',
  'Найди одинаковые файлы',
  'Распознай текст со скана',
];

export default function AssistantPage() {
  const { files, toast, refreshQueue } = useStore();
  const [query, setQuery] = useState('');
  const [reply, setReply] = useState<AssistantReplyInfo | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [accepted, setAccepted] = useState(false);

  const ask = async (q?: string) => {
    const question = (q ?? query).trim();
    if (!question) return;
    setBusy(true); setError(''); setReply(null); setAccepted(false);
    try {
      const paths = files.map((f) => f.path);
      setReply(await api2.assistantAsk(question, paths));
    } catch (e) { setError(String(e)); }
    setBusy(false);
  };

  const applyTasks = async () => {
    if (!reply || reply.proposed_tasks.length === 0) return;
    // Задачи из assistant содержат payload с kind — ставим в очередь.
    try {
      for (const task of reply.proposed_tasks) {
        const payload = JSON.parse(task.payload_json);
        await window.__TAURI_INTERNALS__!.invoke('enqueue', { kind: payload, priority: 'Normal' });
      }
      await refreshQueue();
      setAccepted(true);
      toast('success', `Задач в очереди: ${reply.proposed_tasks.length}`);
    } catch (e) { setError(String(e)); }
  };

  return (
    <Page title="AI-помощник" subtitle="Локальный ассистент: объясняет, подсказывает настройки и предлагает задачи — без облака">
      <div className="card p-5">
        <div className="flex gap-3">
          <input
            className="input flex-1"
            placeholder="Что сделать с файлами? Например: «уменьши размер видео»"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && ask()}
          />
          <button className="btn-primary" onClick={() => ask()} disabled={busy || !query.trim()}>
            <span className="flex items-center gap-2"><Send size={15} /> Спросить</span>
          </button>
        </div>
        <div className="flex flex-wrap gap-2 mt-3">
          {EXAMPLES.map((ex) => (
            <button
              key={ex}
              className="badge bg-slate-100 dark:bg-slate-800 hover:bg-accent/10 hover:text-accent transition-colors cursor-pointer"
              onClick={() => { setQuery(ex); ask(ex); }}
            >
              {ex}
            </button>
          ))}
        </div>
        {files.length > 0 && (
          <div className="text-xs text-slate-400 mt-3">
            Учту выбранные файлы ({files.length}): {files.map((f) => f.name).join(', ')}
          </div>
        )}
      </div>

      <ErrorBox text={error} />

      {reply && (
        <div className="card p-5 space-y-4">
          <div className="flex items-center gap-2">
            <Sparkles size={16} className="text-accent" />
            <span className="font-medium">{reply.intent}</span>
          </div>
          <p className="text-sm">{reply.explanation}</p>

          {reply.steps.length > 0 && (
            <div>
              <div className="text-xs text-slate-400 mb-1">Пошагово:</div>
              <ol className="text-sm space-y-1 list-decimal list-inside">
                {reply.steps.map((s, i) => <li key={i}>{s}</li>)}
              </ol>
            </div>
          )}

          {reply.warnings.map((w, i) => (
            <div key={i} className="flex items-start gap-2 text-sm text-amber-500 bg-amber-400/10 rounded-xl p-3">
              <AlertTriangle size={15} className="mt-0.5 shrink-0" /> {w}
            </div>
          ))}

          {reply.proposed_tasks.length > 0 && (
            <div>
              <div className="text-xs text-slate-400 mb-2">Готовые задачи (нажмите, чтобы поставить в очередь — это подтверждение):</div>
              <div className="space-y-2">
                {reply.proposed_tasks.map((t, i) => (
                  <div key={i} className="text-sm flex items-center gap-2 bg-slate-100 dark:bg-slate-800 rounded-xl px-3 py-2">
                    <CheckCircle2 size={14} className="text-emerald-500 shrink-0" /> {t.label}
                  </div>
                ))}
              </div>
              {!accepted ? (
                <button className="btn-primary mt-3" onClick={applyTasks} disabled={!tauriAvailable()}>
                  Подтвердить и поставить в очередь ({reply.proposed_tasks.length})
                </button>
              ) : (
                <div className="text-sm text-emerald-500 mt-3">Задачи поставлены в очередь — смотрите главную страницу.</div>
              )}
            </div>
          )}
        </div>
      )}
    </Page>
  );
}
