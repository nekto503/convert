// Мост к Tauri-командам. В браузере (без Tauri) вызовы возвращают
// понятную ошибку — интерфейс остаётся честным: нет фейковых данных.

declare global {
  interface Window {
    __TAURI_INTERNALS__?: { invoke: (cmd: string, args?: unknown) => Promise<unknown> };
  }
}

export function tauriAvailable(): boolean {
  return typeof window !== 'undefined' && !!window.__TAURI_INTERNALS__;
}

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!tauriAvailable()) {
    throw new Error(
      'Команда доступна только в приложении CONVERT (запустите сборку Tauri)',
    );
  }
  return window.__TAURI_INTERNALS__!.invoke(cmd, args) as Promise<T>;
}

// ---- Типы (соответствуют Rust-структурам) --------------------------------

export interface ScanInfo {
  path: string;
  file_name: string;
  detected_format: string;
  category: string;
  extension: string;
  extension_matches: boolean;
  size_bytes: number;
  details: [string, string][];
  targets: { format: string; extension: string; note: string }[];
}

export interface TaskInfo {
  id: number;
  kind: { kind: string; [k: string]: unknown };
  label?: string;
  priority: 'High' | 'Normal' | 'Low';
  status: 'Pending' | 'Running' | 'Done' | 'Error' | 'Cancelled';
  error?: string | null;
  output?: string | null;
}

export interface HistoryItem {
  id: number;
  operation: string;
  tool: string;
  source: string;
  output?: string | null;
  status: string;
  error?: string | null;
  duration_ms: number;
  timestamp: number;
}

export interface AppSettings {
  appearance: { theme: string; accent: string };
  processing: {
    output_directory?: string | null;
    overwrite_results: boolean;
    stop_on_error: boolean;
    cleanup_temp: boolean;
  };
  batch: { max_parallel_tasks: number };
  history: { enabled: boolean; retention_days: number; max_entries: number };
  privacy: { telemetry_enabled: boolean; check_updates: boolean; allow_external_processing: boolean };
}

export interface EngineStatusInfo {
  engine: string;
  available: boolean;
  path?: string | null;
  version?: string | null;
}

// ---- API -------------------------------------------------------------------

export const api = {
  scanFile: (path: string) => invoke<ScanInfo>('scan_file', { path }),
  enqueue: (kind: unknown, priority?: string) =>
    invoke<number>('enqueue', { kind, priority }),
  queueSnapshot: () => invoke<TaskInfo[]>('queue_snapshot'),
  queuePause: () => invoke<void>('queue_pause'),
  queueResume: () => invoke<void>('queue_resume'),
  queueCancel: (id: number) => invoke<boolean>('queue_cancel', { id }),
  queueCancelAll: () => invoke<number>('queue_cancel_all'),
  queueRetry: (id: number) => invoke<number | null>('queue_retry', { id }),
  history: (query?: string, status?: string) =>
    invoke<HistoryItem[]>('history', { query, status, limit: 200 }),
  historyClear: () => invoke<void>('history_clear'),
  getSettings: () => invoke<AppSettings>('get_settings'),
  saveSettings: (settings: AppSettings) => invoke<void>('save_settings', { settings }),
  enginesStatus: () => invoke<EngineStatusInfo[]>('engines_status'),
  imageMetadata: (path: string) => invoke<Record<string, unknown>>('image_metadata', { path }),
  imagePalette: (path: string) => invoke<{ hex: string; share: number }[]>('image_palette', { path, maxColors: 8 }),
  smartCompressAnalyze: (path: string, goal?: string) =>
    invoke<{ variants: { quality: number; estimated_size: number; saving_percent: number }[]; recommended: number; note: string }>(
      'smart_compress_analyze', { path, goal },
    ),
  findDuplicates: (root: string) =>
    invoke<{ hash: string; files: string[]; size: number }[]>('find_duplicates', { root }),
  archiveList: (path: string) =>
    invoke<{ name: string; size: number; is_dir: boolean }[]>('archive_list', { path }),
  pdfMetadata: (path: string) => invoke<Record<string, unknown>>('pdf_metadata', { path }),
  pdfText: (path: string) => invoke<string>('pdf_text', { path }),
  ocrImage: (path: string, langs?: string) =>
    invoke<{ text: string; confidence?: number | null }>('ocr_image', { path, langs }),
  probeMedia: (path: string) => invoke<Record<string, unknown>>('probe_media', { path }),
  presetsList: (tool: string) => invoke<[string, string][]>('presets_list', { tool }),
  presetSave: (name: string, tool: string, settingsJson: string) =>
    invoke<number>('preset_save', { name, tool, settingsJson }),
  presetDelete: (name: string) => invoke<void>('preset_delete', { name }),
  workflowsList: () => invoke<[string, string][]>('workflows_list'),
  workflowSave: (name: string, stepsJson: string) => invoke<number>('workflow_save', { name, stepsJson }),
  workflowDelete: (name: string) => invoke<void>('workflow_delete', { name }),
  showInFolder: (path: string) => invoke<void>('show_in_folder', { path }),
};

export interface CompareReportInfo {
  kind: 'image' | 'media' | 'text' | 'generic';
  identical: boolean;
  left_size: number;
  right_size: number;
  size_change_percent: number;
  pixel_difference_percent?: number | null;
  left_preview?: string | null;
  right_preview?: string | null;
  text_diff: { change: 'equal' | 'added' | 'removed'; text: string }[];
  properties: [string, string, string][];
}

export interface AssistantReplyInfo {
  intent: string;
  explanation: string;
  steps: string[];
  suggested_settings_json?: string | null;
  proposed_tasks: { label: string; payload_json: string }[];
  warnings: string[];
}

export interface WorkflowStepInfo {
  op: 'scan' | 'compress' | 'convert_image' | 'resize' | 'ocr' | 'archive';
  [k: string]: unknown;
}

export interface WatchRuleInfo {
  id: number;
  name: string;
  folder: string;
  extensions: string[];
  action: { action: string; [k: string]: unknown };
  auto: boolean;
  enabled: boolean;
}

export interface WatchEventInfo {
  rule_id: number;
  path: string;
  suggestion: string;
}

export const api2 = {
  compareFiles: (left: string, right: string) => invoke<CompareReportInfo>('compare_files', { left, right }),
  assistantAsk: (query: string, files?: string[]) => invoke<AssistantReplyInfo>('assistant_ask', { query, files }),
  enhanceImage: (path: string, output?: string, level?: string) =>
    invoke<{ output_path: string; disclaimer: string; applied_operations: string[] }>('enhance_image', { path, output, level }),
  workflowRun: (name: string, inputs: string[]) =>
    invoke<{ inputs: string[]; outputs: string[]; step_logs: string[]; success: boolean }>('workflow_run', { name, inputs }),
  watchRules: () => invoke<WatchRuleInfo[]>('watch_rules'),
  watchUpsert: (rule: WatchRuleInfo) => invoke<void>('watch_upsert', { rule }),
  watchRemove: (id: number) => invoke<void>('watch_remove', { id }),
  watchSetEnabled: (id: number, enabled: boolean) => invoke<void>('watch_set_enabled', { id, enabled }),
  watchEvents: () => invoke<WatchEventInfo[]>('watch_events'),
  watchConfirm: (event: WatchEventInfo) => invoke<number>('watch_confirm', { event }),
  startupFile: () => invoke<string | null>('startup_file'),
  imageMetadataEdit: (path: string, artist?: string, copyright?: string, description?: string, strip?: boolean) =>
    invoke<void>('image_metadata_edit', { path, artist, copyright, description, strip }),
  imageStripMetadata: (path: string) => invoke<void>('image_strip_metadata', { path }),
};
