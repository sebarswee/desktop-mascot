import { invoke } from '@tauri-apps/api/core';

interface BehaviorConfig {
  idle_weight: number;
  walk_weight: number;
  peek_weight: number;
  disappear_weight: number;
  interact_weight: number;
  show_in_dock: boolean;
  show_in_menu_bar: boolean;
  fixed_corner: string | null;
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

  return {
    idle_weight: getNum('idle-weight'),
    walk_weight: getNum('walk-weight'),
    peek_weight: getNum('peek-weight'),
    disappear_weight: getNum('disappear-weight'),
    interact_weight: getNum('interact-weight'),
    show_in_dock: getBool('show-in-dock'),
    show_in_menu_bar: getBool('show-in-menu-bar'),
    fixed_corner: getSelect('fixed-corner'),
  };
}

function setFormValues(config: BehaviorConfig) {
  const setVal = (id: string, val: string | number | boolean | null) => {
    const el = document.getElementById(id) as HTMLInputElement | HTMLSelectElement;
    if (!el) return;
    if (el.type === 'checkbox') {
      (el as HTMLInputElement).checked = val as boolean;
    } else if (el.tagName === 'SELECT') {
      (el as HTMLSelectElement).value = val === null ? '' : String(val);
    } else {
      (el as HTMLInputElement).value = String(val ?? '');
    }
  };

  setVal('idle-weight', config.idle_weight);
  setVal('walk-weight', config.walk_weight);
  setVal('peek-weight', config.peek_weight);
  setVal('disappear-weight', config.disappear_weight);
  setVal('interact-weight', config.interact_weight);
  setVal('show-in-dock', config.show_in_dock);
  setVal('show-in-menu-bar', config.show_in_menu_bar);
  setVal('fixed-corner', config.fixed_corner);
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
    console.log('[Settings] Loaded:', currentConfig);
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
    console.log('[Settings] Saved:', config);
  } catch (e) {
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

// Bind buttons
document.getElementById('save-btn')?.addEventListener('click', saveSettings);
document.getElementById('reset-btn')?.addEventListener('click', resetSettings);

loadSettings();
