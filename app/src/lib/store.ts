// Глобальное состояние (zustand): тема, файлы, уведомления, очередь.

import { create } from 'zustand';
import { api, tauriAvailable, type TaskInfo } from './api';

export interface DroppedFile {
  path: string;
  name: string;
  size: number;
}

export interface Toast {
  id: number;
  kind: 'info' | 'success' | 'error';
  text: string;
}

interface Store {
  theme: 'light' | 'dark' | 'system';
  accent: string;
  files: DroppedFile[];
  queue: TaskInfo[];
  toasts: Toast[];
  setTheme: (t: 'light' | 'dark' | 'system') => void;
  setAccent: (c: string) => void;
  addFiles: (files: DroppedFile[]) => void;
  clearFiles: () => void;
  removeFile: (path: string) => void;
  toast: (kind: Toast['kind'], text: string) => void;
  dismissToast: (id: number) => void;
  refreshQueue: () => Promise<void>;
}

export const useStore = create<Store>((set, get) => ({
  theme: (localStorage.getItem('convert-theme') as 'light' | 'dark' | 'system') || 'system',
  accent: localStorage.getItem('convert-accent') || '#4F7CFF',
  files: [],
  queue: [],
  toasts: [],

  setTheme: (theme) => {
    localStorage.setItem('convert-theme', theme);
    applyTheme(theme);
    set({ theme });
  },
  setAccent: (accent) => {
    localStorage.setItem('convert-accent', accent);
    document.documentElement.style.setProperty('--accent', hexToRgb(accent));
    set({ accent });
  },

  addFiles: (files) => {
    const existing = new Set(get().files.map((f) => f.path));
    set({ files: [...get().files, ...files.filter((f) => !existing.has(f.path))] });
  },
  clearFiles: () => set({ files: [] }),
  removeFile: (path) => set({ files: get().files.filter((f) => f.path !== path) }),

  toast: (kind, text) => {
    const id = Date.now() + Math.random();
    set({ toasts: [...get().toasts, { id, kind, text }] });
    setTimeout(() => get().dismissToast(id), 5000);
  },
  dismissToast: (id) => set({ toasts: get().toasts.filter((t) => t.id !== id) }),

  refreshQueue: async () => {
    if (!tauriAvailable()) return;
    try {
      set({ queue: await api.queueSnapshot() });
    } catch {
      /* приложение ещё не готово */
    }
  },
}));

export function hexToRgb(hex: string): string {
  const m = hex.replace('#', '');
  const n = parseInt(m.length === 3 ? m.split('').map((c) => c + c).join('') : m, 16);
  return `${(n >> 16) & 255} ${(n >> 8) & 255} ${n & 255}`;
}

export function applyTheme(theme: 'light' | 'dark' | 'system') {
  const dark =
    theme === 'dark' ||
    (theme === 'system' &&
      window.matchMedia('(prefers-color-scheme: dark)').matches);
  document.documentElement.classList.toggle('dark', dark);
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} Б`;
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} КБ`;
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} МБ`;
  return `${(n / 1024 ** 3).toFixed(2)} ГБ`;
}
