// Workflow Builder (ТЗ §5.3) + Автосценарии (ТЗ §5.5).
// Конструктор цепочек шагов, сохранение в БД, запуск; watch-правила.
import { useEffect, useState } from 'react';
import { Play, Save, Trash2, Plus, FolderClock, Eye, EyeOff } from 'lucide-react';
import { api, api2, tauriAvailable, type WorkflowStepInfo, type WatchRuleInfo, type WatchEventInfo } from '../lib/api';
import { useStore } from '../lib/store';
import { Page, ErrorBox, DropZone, FileList } from '../components/ui';

interface StepTemplate {
  op: WorkflowStepInfo['op'];
  label: string;
  make: () => WorkflowStepInfo;
}

const STEP_TEMPLATES: StepTemplate[] = [
  { op: 'scan', label: 'Фильтр форматов (условие)', make: () => ({ op: 'scan', only_formats: ['png', 'jpg'] }) },
  { op: 'compress', label: 'Сжатие изображений', make: () => ({ op: 'compress', quality: 80 }) },
  { op: 'convert_image', label: 'Конвертация изображений', make: () => ({ op: 'convert_image', format: 'png', quality: 90 }) },
  { op: 'resize', label: 'Изменение размера', make: () => ({ op: 'resize', width: 1024 }) },
  { op: 'ocr', label: 'Распознать текст (OCR)', make: () => ({ op: 'ocr', langs: 'eng' }) },
  { op: 'archive', label: 'Упаковать в ZIP', make: () => ({ op: 'archive', name: 'results' }) },
];

export default function WorkflowsPage() {
  const { files, toast, refreshQueue } = useStore();
  const [name, setName] = useState('');
  const [steps, setSteps] = useState<WorkflowStepInfo[]>([]);
  const [saved, setSaved] = useState<[string, string][]>([]);
  const [log, setLog] = useState<string[]>([]);
  const [error, setError] = useState('');
  const [watchRules, setWatchRules] = useState<WatchRuleInfo[]>([]);
  const [watchEvents, setWatchEvents] = useState<WatchEventInfo[]>([]);

  const reload = async () => {
    if (!tauriAvailable()) return;
    try {
      setSaved(await api.workflowsList());
      setWatchRules(await api2.watchRules());
      setWatchEvents(await api2.watchEvents());
    } catch { /* приложение не готово */ }
  };
  useEffect(() => { reload(); }, []); // eslint-disable-line
  useEffect(() => {
    const t = setInterval(() => {
      if (tauriAvailable()) api2.watchEvents().then(setWatchEvents).catch(() => {});
    }, 5000);
    return () => clearInterval(t);
  }, []);

  const saveWorkflow = async () => {
    if (!name.trim() || steps.length === 0) { setError('Укажите имя и добавьте шаги'); return; }
    setError('');
    try {
      await api.workflowSave(name.trim(), JSON.stringify({ name: name.trim(), steps }));
      await reload();
      toast('success', `Сценарий «${name.trim()}» сохранён`);
    } catch (e) { setError(String(e)); }
  };

  const runWorkflow = async (wfName: string) => {
    if (files.length === 0) { setError('Сначала добавьте файлы для обработки'); return; }
    setError(''); setLog([]);
    try {
      const report = await api2.workflowRun(wfName, files.map((f) => f.path));
      setLog(report.step_logs);
      toast('success', `Готово: ${report.outputs.length} результатов`);
      await reload();
    } catch (e) { setError(String(e)); }
  };

  return (
    <Page title="Сценарии и автозапуск" subtitle="Цепочки операций (Workflow) и слежение за папками (автосценарии)">
      {/* --- Конструктор --- */}
      <div className="card p-5 space-y-4">
        <div className="font-medium">Конструктор сценария</div>
        <div className="flex gap-3">
          <input className="input" placeholder="Имя сценария (например: «веб-публикация фото»)" value={name} onChange={(e) => setName(e.target.value)} />
          <button className="btn-primary shrink-0" onClick={saveWorkflow} disabled={!tauriAvailable()}>
            <span className="flex items-center gap-2"><Save size={15} /> Сохранить</span>
          </button>
        </div>
        <div className="flex flex-wrap gap-2">
          {STEP_TEMPLATES.map((t) => (
            <button key={t.op} className="badge bg-slate-100 dark:bg-slate-800 hover:bg-accent/10 hover:text-accent cursor-pointer transition-colors"
              onClick={() => setSteps([...steps, t.make()])}>
              <Plus size={12} /> {t.label}
            </button>
          ))}
        </div>
        {steps.length > 0 && (
          <div className="space-y-2">
            {steps.map((s, i) => (
              <div key={i} className="flex items-center gap-3 bg-slate-100 dark:bg-slate-800 rounded-xl px-3 py-2 text-sm">
                <span className="badge bg-accent/10 text-accent">{i + 1}</span>
                <span className="flex-1">{describeStep(s)}</span>
                <StepEditor step={s} onChange={(ns) => setSteps(steps.map((x, j) => (j === i ? ns : x)))} />
                <button className="text-slate-400 hover:text-red-400" onClick={() => setSteps(steps.filter((_, j) => j !== i))}>
                  <Trash2 size={14} />
                </button>
              </div>
            ))}
            <div className="text-xs text-slate-400">Файлы проходят шаги по порядку; результат каждого шага — вход следующего.</div>
          </div>
        )}
      </div>

      {/* --- Сохранённые сценарии --- */}
      <div className="card p-5">
        <div className="font-medium mb-3">Сохранённые сценарии</div>
        {saved.length === 0 && <div className="text-sm text-slate-400">Пока пусто — создайте сценарий выше</div>}
        <div className="space-y-2">
          {saved.map(([wfName]) => (
            <div key={wfName} className="flex items-center gap-3 text-sm">
              <span className="flex-1">{wfName}</span>
              <button className="btn-ghost text-xs" onClick={() => runWorkflow(wfName)} disabled={!tauriAvailable() || files.length === 0}>
                <span className="flex items-center gap-1"><Play size={12} /> Запустить на выбранных файлах</span>
              </button>
              <button className="btn-ghost text-xs text-red-400" onClick={async () => {
                await api.workflowDelete(wfName); reload();
              }}>
                <Trash2 size={12} />
              </button>
            </div>
          ))}
        </div>
      </div>

      <DropZone hint="файлы для запуска сценария" />
      <FileList />

      {log.length > 0 && (
        <div className="card p-4 text-sm font-mono space-y-1">
          {log.map((l, i) => <div key={i} className="text-slate-500 dark:text-slate-400">{l}</div>)}
        </div>
      )}

      {/* --- Автосценарии --- */}
      <div className="card p-5 space-y-4">
        <div className="flex items-center gap-2 font-medium">
          <FolderClock size={16} className="text-accent" /> Автосценарии (наблюдение за папками)
        </div>
        <WatchRulesEditor rules={watchRules} onChanged={reload} />
        {watchEvents.length > 0 && (
          <div className="space-y-2">
            <div className="text-xs text-slate-400">Новые файлы (подтвердите обработку):</div>
            {watchEvents.map((e, i) => (
              <div key={i} className="flex items-center gap-3 text-sm bg-amber-400/10 rounded-xl px-3 py-2">
                <span className="flex-1">{e.suggestion}</span>
                <button className="btn-primary text-xs" onClick={async () => {
                  await api2.watchConfirm(e);
                  setWatchEvents((ev) => ev.filter((_, j) => j !== i));
                  toast('success', 'Задача поставлена в очередь');
                  refreshQueue();
                }}>
                  Обработать
                </button>
              </div>
            ))}
          </div>
        )}
      </div>

      <ErrorBox text={error} />
    </Page>
  );
}

function describeStep(s: WorkflowStepInfo): string {
  switch (s.op) {
    case 'scan': return `Фильтр: только ${(s.only_formats as string[]).join(', ')}`;
    case 'compress': return `Сжатие (качество ${s.quality})`;
    case 'convert_image': return `Конвертация в ${String(s.format).toUpperCase()} (${s.quality})`;
    case 'resize': return `Ширина ${s.width} px`;
    case 'ocr': return `OCR (${s.langs})`;
    case 'archive': return `ZIP «${s.name}»`;
  }
}

function StepEditor({ step, onChange }: { step: WorkflowStepInfo; onChange: (s: WorkflowStepInfo) => void }) {
  const num = (v: unknown) => Number(v ?? 0);
  switch (step.op) {
    case 'scan':
      return (
        <input className="input max-w-48 text-xs" defaultValue={(step.only_formats as string[]).join(',')}
          onBlur={(e) => onChange({ ...step, only_formats: e.target.value.split(',').map((s) => s.trim()).filter(Boolean) })} />
      );
    case 'compress':
      return (
        <input type="range" min={30} max={95} defaultValue={num(step.quality)}
          onMouseUp={(e) => onChange({ ...step, quality: Number((e.target as HTMLInputElement).value) })}
          onTouchEnd={(e) => onChange({ ...step, quality: Number((e.target as HTMLInputElement).value) })} className="w-24" />
      );
    case 'convert_image':
      return (
        <select className="input max-w-24 text-xs" defaultValue={String(step.format)}
          onChange={(e) => onChange({ ...step, format: e.target.value })}>
          {['png', 'jpg', 'webp', 'bmp', 'tiff'].map((f) => <option key={f}>{f}</option>)}
        </select>
      );
    case 'resize':
      return (
        <input className="input max-w-24 text-xs" type="number" defaultValue={num(step.width)}
          onBlur={(e) => onChange({ ...step, width: Number(e.target.value) || 1024 })} />
      );
    case 'ocr':
      return (
        <select className="input max-w-28 text-xs" defaultValue={String(step.langs)}
          onChange={(e) => onChange({ ...step, langs: e.target.value })}>
          <option value="eng">eng</option>
          <option value="rus">rus</option>
          <option value="rus+eng">rus+eng</option>
        </select>
      );
    case 'archive':
      return (
        <input className="input max-w-32 text-xs" defaultValue={String(step.name)}
          onBlur={(e) => onChange({ ...step, name: e.target.value || 'results' })} />
      );
  }
}

function WatchRulesEditor({ rules, onChanged }: { rules: WatchRuleInfo[]; onChanged: () => void }) {
  const { toast } = useStore();
  const [folder, setFolder] = useState('');
  const [ruleName, setRuleName] = useState('');

  const addRule = async () => {
    if (!folder.trim()) return;
    // Папка указывается путём (drag&drop файла из папки даёт путь).
    const dir = folder.replace(/[^\\/]+$/, '');
    const rule: WatchRuleInfo = {
      id: Date.now(),
      name: ruleName.trim() || `Правило ${rules.length + 1}`,
      folder: dir,
      extensions: ['png', 'jpg', 'jpeg', 'webp'],
      action: { action: 'convert_images', format: 'png', quality: 85 },
      auto: false,
      enabled: true,
    };
    await api2.watchUpsert(rule);
    setFolder(''); setRuleName('');
    onChanged();
    toast('success', 'Правило добавлено — новые файлы будут предлагаться к обработке');
  };

  return (
    <div className="space-y-3">
      <div className="flex gap-3">
        <input className="input" placeholder="Папка наблюдения (полный путь, например C:\Screenshots\)" value={folder} onChange={(e) => setFolder(e.target.value)} />
        <input className="input max-w-48" placeholder="Имя правила" value={ruleName} onChange={(e) => setRuleName(e.target.value)} />
        <button className="btn-ghost shrink-0" onClick={addRule} disabled={!tauriAvailable() || !folder.trim()}>
          <span className="flex items-center gap-2"><Plus size={14} /> Добавить</span>
        </button>
      </div>
      {rules.length === 0 && <div className="text-sm text-slate-400">Правил нет</div>}
      {rules.map((r) => (
        <div key={r.id} className="flex items-center gap-3 text-sm bg-slate-100 dark:bg-slate-800 rounded-xl px-3 py-2">
          <button
            onClick={async () => { await api2.watchSetEnabled(r.id, !r.enabled); onChanged(); }}
            className={r.enabled ? 'text-accent' : 'text-slate-400'}
            title={r.enabled ? 'выключить' : 'включить'}
          >
            {r.enabled ? <Eye size={15} /> : <EyeOff size={15} />}
          </button>
          <span className="font-medium min-w-0 truncate">{r.name}</span>
          <span className="text-xs text-slate-400 truncate flex-1">{r.folder} ({r.extensions.join(', ')})</span>
          <button className="text-slate-400 hover:text-red-400" onClick={async () => { await api2.watchRemove(r.id); onChanged(); }}>
            <Trash2 size={14} />
          </button>
        </div>
      ))}
    </div>
  );
}
