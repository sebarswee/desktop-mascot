import { invoke } from '@tauri-apps/api/core';

interface LlmConfig {
  provider: string;
  api_key: string;
  model: string;
  base_url: string | null;
}

interface BehaviorConfig {
  idle_weight: number;
  walk_weight: number;
  peek_weight: number;
  disappear_weight: number;
  interact_weight: number;
  show_in_dock: boolean;
  show_in_menu_bar: boolean;
  fixed_corner: string | null;
  auto_close_chat: boolean;
  chat_shortcut: string;
  auto_start: boolean;
  llm: LlmConfig;
}

let currentConfig: BehaviorConfig | null = null;

function getFormValues(): BehaviorConfig {
  const getNum = (id: string) => {
    const el = document.getElementById(id) as HTMLInputElement;
    return el ? parseInt(el.value, 10) || 0 : 0;
  };
  const getBool = (id: string) => {
    const el = document.getElementById(id) as HTMLInputElement;
    return el ? el.checked : false;
  };
  const getSelect = (id: string): string | null => {
    const el = document.getElementById(id) as HTMLSelectElement;
    if (!el) return null;
    return el.value === '' ? null : el.value;
  };
  const getStr = (id: string): string => {
    const el = document.getElementById(id) as HTMLInputElement;
    return el ? el.value : '';
  };

  return {
    idle_weight: getNum('idle-weight'),
    walk_weight: getNum('walk-weight'),
    peek_weight: getNum('peek-weight'),
    disappear_weight: getNum('disappear-weight'),
    interact_weight: getNum('interact-weight'),
    show_in_dock: getBool('show-in-dock'),
    show_in_menu_bar: getBool('show-in-menu-bar'),
    fixed_corner: getSelect('fixed-corner'),
    auto_close_chat: getBool('auto-close-chat'),
    chat_shortcut: getStr('chat-shortcut'),
    auto_start: getBool('auto-start'),
    llm: {
      provider: getStr('llm-provider') || 'claude',
      api_key: getStr('llm-api-key'),
      model: getStr('llm-model'),
      base_url: getStr('llm-base-url') || null,
    },
  };
}

function setVal(id: string, val: string | number | boolean | null) {
  const el = document.getElementById(id) as HTMLInputElement | HTMLSelectElement;
  if (!el) return;
  if (el.type === 'checkbox') {
    (el as HTMLInputElement).checked = val as boolean;
  } else if (el.tagName === 'SELECT') {
    (el as HTMLSelectElement).value = val === null ? '' : String(val);
  } else {
    (el as HTMLInputElement).value = String(val ?? '');
  }
}

function setFormValues(config: BehaviorConfig) {
  setVal('idle-weight', config.idle_weight);
  setVal('walk-weight', config.walk_weight);
  setVal('peek-weight', config.peek_weight);
  setVal('disappear-weight', config.disappear_weight);
  setVal('interact-weight', config.interact_weight);
  setVal('show-in-dock', config.show_in_dock);
  setVal('show-in-menu-bar', config.show_in_menu_bar);
  setVal('fixed-corner', config.fixed_corner);
  setVal('auto-close-chat', config.auto_close_chat);
  setVal('chat-shortcut', config.chat_shortcut);
  setVal('auto-start', config.auto_start);

  setVal('llm-provider', config.llm.provider);
  setVal('llm-api-key', config.llm.api_key);
  setVal('llm-model', config.llm.model);
  setVal('llm-base-url', config.llm.base_url);

  updateBaseUrlVisibility();
}

function updateBaseUrlVisibility() {
  const provider = (document.getElementById('llm-provider') as HTMLSelectElement)?.value;
  const row = document.getElementById('base-url-row');
  if (!row) return;
  const show = provider === 'claude' || provider === 'openai_compatible' || provider === 'ollama';
  row.classList.toggle('hidden-row', !show);
}

function showSaveStatus(text: string, ok: boolean) {
  const el = document.getElementById('save-status');
  if (!el) return;
  el.textContent = text;
  el.style.color = ok ? '#7ec699' : '#ff7b82';
  el.classList.add('visible');
  setTimeout(() => el.classList.remove('visible'), 2000);
}

async function loadSettings() {
  try {
    currentConfig = await invoke<BehaviorConfig>('get_config');
    setFormValues(currentConfig);
    // Load API key separately from keyring
    const apiKey = await invoke<string>('get_api_key');
    setVal('llm-api-key', apiKey);
    console.log('[Settings] Loaded');
  } catch (e) {
    console.error('[Settings] Load failed:', e);
  }
}

async function saveSettings() {
  const config = getFormValues();
  try {
    await invoke('set_config', { config });
    currentConfig = config;
    showSaveStatus('已保存', true);
    console.log('[Settings] Saved');
  } catch (e: any) {
    showSaveStatus('保存失败', false);
    console.error('[Settings] Save failed:', e);
  }
}

async function resetSettings() {
  try {
    const config = await invoke<BehaviorConfig>('reset_config');
    currentConfig = config;
    setFormValues(config);
    showSaveStatus('已恢复默认', true);
    console.log('[Settings] Reset to default:', config);
  } catch (e) {
    showSaveStatus('恢复失败', false);
    console.error('[Settings] Reset failed:', e);
  }
}

// Shortcut key recording
const shortcutEl = document.getElementById('chat-shortcut') as HTMLInputElement;
const MODIFIER_KEYS = ['Shift', 'Control', 'Alt', 'Meta', 'OS'];

function formatKey(e: KeyboardEvent): string | null {
  const mods: string[] = [];
  if (e.metaKey) mods.push('Cmd');
  if (e.ctrlKey) mods.push('Ctrl');
  if (e.altKey) mods.push('Alt');
  if (e.shiftKey) mods.push('Shift');

  if (MODIFIER_KEYS.includes(e.key)) {
    return null; // wait for main key
  }

  let key = e.key;
  if (key === ' ') key = 'Space';
  else if (key.length === 1) key = key.toUpperCase();

  return [...mods, key].join('+');
}

if (shortcutEl) {
  shortcutEl.addEventListener('focus', () => {
    shortcutEl.dataset.recording = 'true';
    shortcutEl.placeholder = '按下快捷键…';
    shortcutEl.value = '';
  });

  shortcutEl.addEventListener('blur', () => {
    shortcutEl.dataset.recording = 'false';
    shortcutEl.placeholder = '点击后按下快捷键';
    if (!shortcutEl.value) {
      shortcutEl.value = currentConfig?.chat_shortcut ?? '';
    }
  });

  shortcutEl.addEventListener('keydown', (e) => {
    if (shortcutEl.dataset.recording !== 'true') return;
    e.preventDefault();
    e.stopPropagation();

    if (e.key === 'Escape') {
      shortcutEl.blur();
      return;
    }
    if (e.key === 'Backspace' || e.key === 'Delete') {
      shortcutEl.value = '';
      shortcutEl.blur();
      return;
    }

    const combo = formatKey(e);
    if (combo) {
      shortcutEl.value = combo;
      shortcutEl.blur();
    }
  });
}

// Bind buttons
document.getElementById('save-btn')?.addEventListener('click', saveSettings);
document.getElementById('reset-btn')?.addEventListener('click', resetSettings);
document.getElementById('llm-provider')?.addEventListener('change', updateBaseUrlVisibility);

loadSettings();
