import { useEffect } from 'react';
import { Routes, Route, NavLink } from 'react-router-dom';
import {
  LayoutGrid, Image, Video, Music, FileText, FileArchive, ScanText,
  Settings, History, Wrench, Cog, Sparkles, GitCompare, Workflow,
} from 'lucide-react';
import { useStore, applyTheme, hexToRgb } from './lib/store';
import Dashboard from './pages/Dashboard';
import { ImageStudio } from './pages/ImageStudio';
import { VideoStudio } from './pages/VideoStudio';
import { AudioStudio } from './pages/AudioStudio';
import { PdfStudio } from './pages/PdfStudio';
import { DocumentStudio } from './pages/DocumentStudio';
import { OcrStudio } from './pages/OcrStudio';
import { ArchiveManager } from './pages/ArchiveManager';
import Tools from './pages/Tools';
import HistoryPage from './pages/HistoryPage';
import SettingsPage from './pages/SettingsPage';
import ComparePage from './pages/ComparePage';
import AssistantPage from './pages/AssistantPage';
import WorkflowsPage from './pages/WorkflowsPage';

const NAV = [
  { to: '/', icon: LayoutGrid, label: 'Главная' },
  { to: '/image', icon: Image, label: 'Image Studio' },
  { to: '/video', icon: Video, label: 'Video Studio' },
  { to: '/audio', icon: Music, label: 'Audio Studio' },
  { to: '/pdf', icon: FileText, label: 'PDF Studio' },
  { to: '/docs', icon: Cog, label: 'Document Studio' },
  { to: '/ocr', icon: ScanText, label: 'OCR Studio' },
  { to: '/archive', icon: FileArchive, label: 'Архивы' },
  { to: '/compare', icon: GitCompare, label: 'Сравнение' },
  { to: '/assistant', icon: Sparkles, label: 'AI-помощник' },
  { to: '/workflows', icon: Workflow, label: 'Сценарии' },
  { to: '/tools', icon: Wrench, label: 'Инструменты' },
  { to: '/history', icon: History, label: 'История' },
  { to: '/settings', icon: Settings, label: 'Настройки' },
];

export default function App() {
  const { theme, accent, setTheme, setAccent, toasts, dismissToast } = useStore();

  useEffect(() => {
    applyTheme(theme);
    const h = hexToRgb(accent);
    document.documentElement.style.setProperty('--accent', h);
  }, [theme, accent]);

  return (
    <div className="h-full flex">
      {/* Боковая панель */}
      <aside className="w-60 shrink-0 bg-panel border-r border-slate-200/60 dark:border-slate-700/60 flex flex-col">
        <div className="p-5 flex items-center gap-2.5">
          <div className="w-9 h-9 rounded-xl bg-accent grid place-items-center">
            <Sparkles size={18} className="text-white" />
          </div>
          <div>
            <div className="font-bold tracking-wide">CONVERT</div>
            <div className="text-[11px] text-slate-400">универсальный инструмент</div>
          </div>
        </div>
        <nav className="flex-1 px-3 space-y-0.5 overflow-y-auto">
          {NAV.map(({ to, icon: Icon, label }) => (
            <NavLink
              key={to}
              to={to}
              className={({ isActive }) =>
                `flex items-center gap-3 px-3 py-2 rounded-xl text-sm transition-colors ${
                  isActive
                    ? 'bg-accent/10 text-accent font-medium'
                    : 'text-slate-600 dark:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-800'
                }`
              }
            >
              <Icon size={17} />
              {label}
            </NavLink>
          ))}
        </nav>
        <div className="p-3">
          <button
            className="btn-ghost w-full text-xs justify-center flex items-center gap-2"
            onClick={() => setTheme(theme === 'dark' ? 'light' : 'dark')}
          >
            {theme === 'dark' ? '☀️ Светлая тема' : '🌙 Тёмная тема'}
          </button>
        </div>
      </aside>

      {/* Контент */}
      <main className="flex-1 overflow-y-auto">
        <Routes>
          <Route path="/" element={<Dashboard />} />
          <Route path="/image" element={<ImageStudio />} />
          <Route path="/video" element={<VideoStudio />} />
          <Route path="/audio" element={<AudioStudio />} />
          <Route path="/pdf" element={<PdfStudio />} />
          <Route path="/docs" element={<DocumentStudio />} />
          <Route path="/ocr" element={<OcrStudio />} />
          <Route path="/archive" element={<ArchiveManager />} />
          <Route path="/tools" element={<Tools />} />
          <Route path="/history" element={<HistoryPage />} />
          <Route path="/settings" element={<SettingsPage />} />
        </Routes>
      </main>

      {/* Уведомления */}
      <div className="fixed bottom-4 right-4 space-y-2 z-50">
        {toasts.map((t) => (
          <div
            key={t.id}
            onClick={() => dismissToast(t.id)}
            className={`card px-4 py-3 text-sm cursor-pointer shadow-lg max-w-sm ${
              t.kind === 'error' ? 'border-red-400/50' : t.kind === 'success' ? 'border-emerald-400/50' : ''
            }`}
          >
            {t.text}
          </div>
        ))}
      </div>
    </div>
  );
}
