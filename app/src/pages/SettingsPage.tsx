import { useEffect, useState } from 'react';
import { api, tauriAvailable, type AppSettings, type EngineStatusInfo } from '../lib/api';
import { useStore } from '../lib/store';
import { Page } from '../components/ui';

export default function SettingsPage() {
  const { theme, setTheme, accent, setAccent, toast } = useStore();
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [engines, setEngines] = useState<EngineStatusInfo[]>([]);

  useEffect(() => {
    if (tauriAvailable()) {
      api.getSettings().then(setSettings).catch(() => {});
      api.enginesStatus().then(setEngines).catch(() => {});
    }
  }, []);

  const save = async () => {
    if (!tauriAvailable() || !settings) return;
    try {
      await api.saveSettings(settings);
      toast('success', 'Настройки сохранены');
    } catch (e) { toast('error', String(e)); }
  };

  return (
    <Page title="Настройки" subtitle="Внешний вид, обработка, конфиденциальность, компоненты">
      <div className="card p-5 space-y-4">
        <div className="font-medium">Внешний вид</div>
        <div className="flex gap-3 items-center">
          <select className="input max-w-48" value={theme} onChange={(e) => setTheme(e.target.value as 'light' | 'dark' | 'system')}>
            <option value="system">Как в системе</option>
            <option value="light">Светлая</option>
            <option value="dark">Тёмная</option>
          </select>
          <input type="color" value={accent} onChange={(e) => setAccent(e.target.value)} className="w-10 h-10 rounded-lg cursor-pointer" />
          <span className="text-sm text-slate-400">Акцентный цвет</span>
        </div>
      </div>

      {settings && (
        <div className="card p-5 space-y-4">
          <div className="font-medium">Обработка и конфиденциальность</div>
          {([
            ['Обрабатывать файлы в облаке (никогда без согласия)', 'privacy.allow_external_processing', false],
            ['Сохранять историю операций', 'history.enabled', true],
          ] as const).map(([label, path, def]) => {
            const [section, key] = path.split('.');
            const checked = (settings as never as Record<string, Record<string, boolean>>)[section]?.[key] ?? def;
            return (
              <label key={path} className="flex items-center gap-3 text-sm">
                <input
                  type="checkbox"
                  checked={checked}
                  onChange={(e) => setSettings({
                    ...settings,
                    [section]: { ...(settings as never as Record<string, object>)[section], [key]: e.target.checked },
                  })}
                />
                {label}
              </label>
            );
          })}
          <div className="text-xs text-slate-400">
            Телеметрия выключена и никуда не отправляется. Все операции выполняются локально.
          </div>
          <button className="btn-primary" onClick={save}>Сохранить</button>
        </div>
      )}

      <div className="card p-5">
        <div className="font-medium mb-3">Установленные компоненты</div>
        <div className="space-y-2">
          {engines.length === 0 && <div className="text-sm text-slate-400">Список доступен в приложении</div>}
          {engines.map((e) => (
            <div key={e.engine} className="flex items-center gap-3 text-sm">
              <span className={`badge ${e.available ? 'bg-emerald-400/15 text-emerald-500' : 'bg-red-400/15 text-red-400'}`}>
                {e.available ? 'установлен' : 'не найден'}
              </span>
              <span className="font-medium w-20">{e.engine}</span>
              <span className="text-xs text-slate-400 truncate">{e.version || '—'}</span>
            </div>
          ))}
        </div>
      </div>
    </Page>
  );
}
