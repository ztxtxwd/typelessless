const { invoke } = window.__TAURI__.core;
const { event } = window.__TAURI__;

const toastContainer = document.getElementById('toast-container');

function showToast(message) {
  const toast = document.createElement('div');
  toast.className = 'toast';
  toast.textContent = message;
  toast.addEventListener('click', () => toast.remove());
  toastContainer.appendChild(toast);
  setTimeout(() => toast.remove(), 5500);
}

event.listen('app-error', (e) => {
  showToast(e.payload);
});

const audioDeviceEl = document.getElementById('audio-device');
const apiKeyEl = document.getElementById('api-key');
const modelEl = document.getElementById('model');
const languageEl = document.getElementById('language');
const systemPromptEl = document.getElementById('system-prompt');
const systemPromptResetBtn = document.getElementById('system-prompt-reset');
const saveBtn = document.getElementById('save-btn');
const shortcutDisplay = document.getElementById('shortcut-display');
const shortcutAssignBtn = document.getElementById('shortcut-assign-btn');
const shortcutDefaultBtn = document.getElementById('shortcut-default-btn');
const shortcutError = document.getElementById('shortcut-error');

let defaultSystemPrompt = '';

async function loadConfig() {
  try {
    try {
      defaultSystemPrompt = await invoke('get_default_system_prompt');
    } catch (e) {
      console.error('Failed to load default system prompt:', e);
    }

    const config = await invoke('get_config');
    apiKeyEl.value = config.api_key || '';
    modelEl.value = config.model || 'doubao-seed-2-0-lite-260428';
    languageEl.value = config.language || 'auto';
    systemPromptEl.value = config.system_prompt || defaultSystemPrompt;
    shortcutDisplay.value = formatShortcutDisplay(config.shortcut || 'Alt+Space');

    // Add the configured model option if it isn't in the dropdown
    if (![...modelEl.options].some(o => o.value === modelEl.value)) {
      const opt = document.createElement('option');
      opt.value = modelEl.value;
      opt.textContent = modelEl.value;
      modelEl.appendChild(opt);
      modelEl.value = config.model;
    }

    const devices = await invoke('list_audio_devices');
    audioDeviceEl.innerHTML = '<option value="default">Default</option>';
    devices.forEach(d => {
      const opt = document.createElement('option');
      opt.value = d;
      opt.textContent = d;
      audioDeviceEl.appendChild(opt);
    });
    if (config.audio_device && config.audio_device !== 'default') {
      audioDeviceEl.value = config.audio_device;
    }
  } catch (e) {
    console.error('Failed to load config:', e);
  }
}

saveBtn.addEventListener('click', async () => {
  try {
    const current = await invoke('get_config');
    const systemPromptValue = systemPromptEl.value.trim();
    await invoke('save_config', {
      config: {
        audio_device: audioDeviceEl.value,
        language: languageEl.value,
        shortcut: current.shortcut || 'Alt+Space',
        api_key: apiKeyEl.value.trim(),
        model: modelEl.value,
        system_prompt: systemPromptValue === '' ? defaultSystemPrompt : systemPromptValue,
      },
    });
    saveBtn.textContent = 'Saved!';
    setTimeout(() => { saveBtn.textContent = 'Save Settings'; }, 1500);
  } catch (e) {
    console.error('Failed to save:', e);
    saveBtn.textContent = 'Error saving';
    setTimeout(() => { saveBtn.textContent = 'Save Settings'; }, 2000);
  }
});

systemPromptResetBtn.addEventListener('click', () => {
  systemPromptEl.value = defaultSystemPrompt;
});

// ── Shortcut ──

function formatShortcutDisplay(shortcut) {
  return shortcut.replace(/\+/g, ' + ');
}

const MODIFIER_KEYS = new Set([
  'Alt', 'Control', 'Shift', 'Meta',
  'AltLeft', 'AltRight', 'ControlLeft', 'ControlRight',
  'ShiftLeft', 'ShiftRight', 'MetaLeft', 'MetaRight',
]);

function codeToKey(code) {
  if (code.startsWith('Key')) return code.slice(3);
  if (code.startsWith('Digit')) return code.slice(5);
  if (code.startsWith('Numpad')) return 'Num' + code.slice(6);
  if (code.startsWith('Arrow')) return code.slice(5);
  const map = {
    'Backquote': '`', 'Minus': '-', 'Equal': '=',
    'BracketLeft': '[', 'BracketRight': ']', 'Backslash': '\\',
    'Semicolon': ';', 'Quote': "'", 'Comma': ',', 'Period': '.',
    'Slash': '/',
  };
  return map[code] || code;
}

let isListening = false;

function startListening() {
  isListening = true;
  shortcutDisplay.value = 'Press a key combo...';
  shortcutDisplay.classList.add('listening');
  shortcutAssignBtn.textContent = 'Cancel';
  shortcutError.textContent = '';
}

function stopListening() {
  isListening = false;
  shortcutDisplay.classList.remove('listening');
  shortcutAssignBtn.textContent = 'Assign';
}

shortcutAssignBtn.addEventListener('click', () => {
  if (isListening) {
    stopListening();
    invoke('get_config').then(config => {
      shortcutDisplay.value = formatShortcutDisplay(config.shortcut || 'Alt+Space');
    });
  } else {
    startListening();
  }
});

shortcutDefaultBtn.addEventListener('click', async () => {
  stopListening();
  shortcutError.textContent = '';
  try {
    await invoke('change_shortcut', { shortcut: 'Alt+Space' });
    shortcutDisplay.value = formatShortcutDisplay('Alt+Space');
  } catch (e) {
    shortcutError.textContent = String(e);
  }
});

document.addEventListener('keydown', async (e) => {
  if (!isListening) return;
  e.preventDefault();
  e.stopPropagation();

  if (MODIFIER_KEYS.has(e.code) || MODIFIER_KEYS.has(e.key)) return;

  const parts = [];
  if (e.ctrlKey) parts.push('Ctrl');
  if (e.altKey) parts.push('Alt');
  if (e.shiftKey) parts.push('Shift');
  if (e.metaKey) parts.push('Super');
  parts.push(codeToKey(e.code));

  const shortcut = parts.join('+');
  stopListening();
  shortcutDisplay.value = formatShortcutDisplay(shortcut);
  shortcutError.textContent = '';

  try {
    await invoke('change_shortcut', { shortcut });
  } catch (err) {
    shortcutError.textContent = String(err);
  }
});

loadConfig();
