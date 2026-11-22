import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { ArrowRight, Image, Video, Music, FileText, FileArchive, ScanText, Cog, Wrench, Clock, Activity } from 'lucide-react';
import { api, tauriAvailable, type ScanInfo, type HistoryItem } from '../lib/api';
import { useStore, formatBytes } from '../lib/store';
import { DropZone, FileList } from '../components/ui';

const TOOLS = [
  { to: '/image', icon: Image, name: 'Image Studio', desc: 'Конвертация, сжатие, фильтры, палитра, удаление фона' },
  { to: '/video', icon: Video, name: 'Video Studio', desc: 'MP4/MKV/WebM, кодеки, обрезка, кадры, склейка' },
  { to: '/audio', icon: Music, name: 'Audio Studio', desc: 'MP3/WAV/FLAC/Opus, громкость, нормализация' },
  { to: '/pdf', icon: FileText, name: 'PDF Studio', desc: 'Слияние, разделение, водяные знаки, нумерация' },
  { to: '/docs', icon: Cog, name: 'Document Studio', desc: 'DOCX/ODT/XLSX/PPTX через LibreOffice' },
  { to: '/ocr', icon: ScanText, name: 'OCR Studio', desc: 'Распознавание текста (Tesseract, локально)' },
  { to: '/archive', icon: FileArchive, name: 'Архивы', desc: 'ZIP/7Z/TAR.GZ, пароли, безопасная распаковка' },
  { to: '/tools', icon: Wrench, name: 'Инструменты', desc: 'Дубликаты, QR, кодировки, Smart Compress' },
];

export default function Dashboard() {
  const { files, refreshQueue, queue } = useStore();
  const [scan, setScan] = useState<ScanInfo | null>(null);
  const [history, setHistory] = useState<HistoryItem[]>([]);

  useEffect(() => {
    const t = setInterval(refreshQueue, 1500);
    refreshQueue();
    if (tauriAvailable()) api.history().then(setHistory).catch(() => {});
    return () => clearInterval(t);
  }, [refreshQueue]);

  useEffect(() => {
    if (files.length === 1 && tauriAvailable()) {
      api.scanFile(files[0].path).then(setScan).catch(() => setScan(null));
    } else {
      setScan(null);
    }
  }, [files]);

  const active = queue.filter((t) => t.status === 'Running' || t.status === 'Pending');

  return (
    <div className="p-6 max-w-6xl mx-auto space-y-6">
      <div>
        <h1 className="text-3xl font-bold">CONVERT</h1>
        <p className="text-slate-400 mt-1">Один инструмент для всех операций с файлами</p>
      </div>

      <DropZone hint="изображения, видео, аудио, документы, PDF, архивы" />
      <FileList />

      {scan && (
        <div className="card p-5">
          <div className="flex items-center gap-3 flex-wrap">
            <span className="badge bg-accent/10 text-accent">{scan.detected_format}</span>
            <span className="text-sm">{scan.category}</span>
            <span className="text-sm text-slate-400">{formatBytes(scan.size_bytes)}</span>
            {!scan.extension_matches && (
              <span className="badge bg-amber-400/15 text-amber-500">
                расширение .{scan.extension} не соответствует содержимому
              </span>
            )}
          </div>
          <div className="mt-3 text-sm text-slate-500">Доступные конвертации:</div>
          <div className="flex flex-wrap gap-2 mt-2">
            {scan.targets.map((t) => (
              <span key={t.extension} className="badge bg-slate-100 dark:bg-slate-800" title={t.note}>
                → {t.format}
              </span>
            ))}
          </div>
        </div>
      )}

      <div>
        <h2 className="font-semibold mb-3">Инструменты</h2>
        <div className="grid sm:grid-cols-2 lg:grid-cols-4 gap-3">
          {TOOLS.map(({ to, icon: Icon, name, desc }) => (
            <Link key={to} to={to} className="card p-4 hover:border-accent/50 transition-colors group">
              <Icon size={22} className="text-accent mb-2" />
              <div className="font-medium text-sm">{name}</div>
              <div className="text-xs text-slate-400 mt-1">{desc}</div>
              <ArrowRight size={14} className="mt-2 text-slate-300 group-hover:text-accent transition-colors" />
            </Link>
          ))}
        </div>
      </div>

      <div className="grid md:grid-cols-2 gap-4">
        <div className="card p-4">
          <div className="flex items-center gap-2 font-medium text-sm mb-3">
            <Activity size={15} className="text-accent" /> Активные задачи ({active.length})
          </div>
          {active.length === 0 ? (
            <div className="text-sm text-slate-400">Очередь пуста</div>
          ) : (
            active.slice(0, 5).map((t) => (
              <div key={t.id} className="text-sm py-1 flex justify-between">
                <span className="truncate">Задача #{t.id}</span>
                <span className={t.status === 'Running' ? 'text-accent' : 'text-slate-400'}>{t.status === 'Running' ? 'выполняется' : 'ожидает'}</span>
              </div>
            ))
          )}
        </div>
        <div className="card p-4">
          <div className="flex items-center gap-2 font-medium text-sm mb-3">
            <Clock size={15} className="text-accent" /> Последние операции
          </div>
          {history.length === 0 ? (
            <div className="text-sm text-slate-400">История пуста</div>
          ) : (
            history.slice(0, 5).map((h) => (
              <div key={h.id} className="text-sm py-1 flex justify-between gap-2">
                <span className="truncate">{h.operation}</span>
                <span className={h.status === 'success' ? 'text-emerald-500' : 'text-red-400'}>
                  {h.status === 'success' ? 'успех' : 'ошибка'}
                </span>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
