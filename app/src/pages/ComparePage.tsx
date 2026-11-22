// File Compare (ТЗ §7): до/после со слайдером, diff текстов, свойства.
import { useState, useRef } from 'react';
import { GitCompare } from 'lucide-react';
import { api2, tauriAvailable, type CompareReportInfo } from '../lib/api';
import { useStore, formatBytes } from '../lib/store';
import { Page, ErrorBox } from '../components/ui';

export default function ComparePage() {
  const { files, toast } = useStore();
  const [report, setReport] = useState<CompareReportInfo | null>(null);
  const [error, setError] = useState('');
  const [slider, setSlider] = useState(50);
  const [busy, setBusy] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  const run = async () => {
    if (!tauriAvailable() || files.length < 2) return;
    setBusy(true); setError(''); setReport(null);
    try {
      setReport(await api2.compareFiles(files[0].path, files[1].path));
    } catch (e) { setError(String(e)); }
    setBusy(false);
  };

  const mode = files.length >= 2 ? 'two-files' : 'manual';

  return (
    <Page title="Сравнение файлов" subtitle="До/после со слайдером для изображений, diff текстов, технические характеристики">
      <div className="card p-4 text-sm text-slate-400">
        Добавьте <b>два файла</b> (первый — «до», второй — «после») через drag & drop на главной,
        затем нажмите «Сравнить». {files.length > 0 && `Сейчас выбрано: ${files.length}.`}
      </div>
      {mode === 'manual' && <ManualPaths onCompare={run} busy={busy} />}
      <ErrorBox text={error} />

      {report && (
        <>
          <div className="card p-4 grid sm:grid-cols-4 gap-3 text-sm">
            <div>
              <div className="text-slate-400 text-xs">Размер 1</div>
              <div className="font-medium">{formatBytes(report.left_size)}</div>
            </div>
            <div>
              <div className="text-slate-400 text-xs">Размер 2</div>
              <div className="font-medium">{formatBytes(report.right_size)}</div>
            </div>
            <div>
              <div className="text-slate-400 text-xs">Изменение</div>
              <div className={`font-medium ${report.size_change_percent <= 0 ? 'text-emerald-500' : 'text-amber-500'}`}>
                {report.size_change_percent > 0 ? '+' : ''}{report.size_change_percent.toFixed(1)}%
              </div>
            </div>
            <div>
              <div className="text-slate-400 text-xs">Содержимое</div>
              <div className="font-medium">{report.identical ? 'идентично' : 'различается'}</div>
            </div>
          </div>

          {report.kind === 'image' && report.left_preview && report.right_preview && (
            <div className="card p-4">
              <div className="font-medium text-sm mb-3">До / после</div>
              <div
                ref={containerRef}
                className="relative overflow-hidden rounded-xl bg-slate-100 dark:bg-slate-800 select-none"
                style={{ maxHeight: 420 }}
              >
                <img src={report.right_preview} alt="после" className="w-full block" draggable={false} />
                <div
                  className="absolute inset-0 overflow-hidden"
                  style={{ width: `${slider}%` }}
                >
                  <img
                    src={report.left_preview}
                    alt="до"
                    className="absolute left-0 top-0 h-full"
                    style={{ width: containerRef.current?.clientWidth ?? '100%' }}
                    draggable={false}
                  />
                </div>
                <div
                  className="absolute top-0 bottom-0 w-0.5 bg-accent pointer-events-none"
                  style={{ left: `${slider}%` }}
                />
              </div>
              <input
                type="range" min={0} max={100} value={slider}
                onChange={(e) => setSlider(+e.target.value)}
                className="w-full mt-3"
              />
              {report.pixel_difference_percent != null && (
                <div className="text-xs text-slate-400 mt-1">
                  Различие пикселей: {report.pixel_difference_percent.toFixed(1)}%
                </div>
              )}
            </div>
          )}

          {report.properties.length > 0 && (
            <div className="card p-4">
              <div className="font-medium text-sm mb-2">Характеристики</div>
              <div className="text-sm divide-y divide-slate-100 dark:divide-slate-800">
                {report.properties.map(([k, l, r]) => (
                  <div key={k} className="py-2 grid grid-cols-3 gap-2">
                    <span className="text-slate-400">{k}</span>
                    <span className="truncate">{l}</span>
                    <span className="truncate">{r}</span>
                  </div>
                ))}
              </div>
            </div>
          )}

          {report.kind === 'text' && report.text_diff.length > 0 && (
            <div className="card p-4 max-h-96 overflow-y-auto font-mono text-xs">
              {report.text_diff.map((line, i) => (
                <div
                  key={i}
                  className={
                    line.change === 'added'
                      ? 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 px-2'
                      : line.change === 'removed'
                        ? 'bg-red-500/10 text-red-600 dark:text-red-400 px-2'
                        : 'px-2'
                  }
                >
                  {line.change === 'added' ? '+ ' : line.change === 'removed' ? '− ' : '  '}
                  {line.text || ' '}
                </div>
              ))}
            </div>
          )}
          <button
            className="btn-ghost text-sm"
            onClick={() => {
              toast('info', 'Сравнение доступно в приложении');
              setReport(null);
            }}
          >
            <span className="flex items-center gap-2"><GitCompare size={14} /> Сбросить</span>
          </button>
        </>
      )}
    </Page>
  );
}

function ManualPaths({ onCompare, busy }: { onCompare: () => void; busy: boolean }) {
  return (
    <div className="card p-4 flex gap-3 items-center">
      <div className="text-sm text-slate-400 flex-1">Для сравнения добавьте два файла drag & drop</div>
      <button className="btn-primary" onClick={onCompare} disabled={busy}>
        {busy ? 'Сравниваю...' : 'Сравнить'}
      </button>
    </div>
  );
}
