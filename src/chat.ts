import { invoke } from '@tauri-apps/api/core';

const messagesEl = document.getElementById('messages')!;
const inputEl = document.getElementById('chat-input') as HTMLInputElement;
const sendBtn = document.getElementById('send-btn')! as HTMLButtonElement;
const closeBtn = document.getElementById('close-btn')!;

let isLoading = false;

function appendMessage(role: 'user' | 'ai' | 'error', content: string) {
  const div = document.createElement('div');
  div.className = `msg-${role}`;
  div.textContent = content;
  messagesEl.appendChild(div);
  messagesEl.scrollTop = messagesEl.scrollHeight;
}

function setLoading(loading: boolean) {
  isLoading = loading;
  sendBtn.disabled = loading;
  inputEl.disabled = loading;
  sendBtn.textContent = loading ? '...' : '发送';
  if (!loading) inputEl.focus();
}

async function sendMessage() {
  const text = inputEl.value.trim();
  if (!text || isLoading) return;

  appendMessage('user', text);
  inputEl.value = '';
  setLoading(true);

  try {
    const reply = await invoke<string>('chat_send', { message: text });
    appendMessage('ai', reply);
  } catch (e: any) {
    appendMessage('error', String(e));
  } finally {
    setLoading(false);
  }
}

sendBtn.addEventListener('click', sendMessage);
inputEl.addEventListener('keydown', (e) => {
  if (e.key === 'Enter') sendMessage();
});
closeBtn.addEventListener('click', () => {
  invoke('close_chat_window').catch(console.error);
});

// Auto-close when clicking outside the chat window (blur), if enabled in settings
async function setupAutoClose() {
  try {
    const config = await invoke<any>('get_config');
    if (config.auto_close_chat !== false) {
      window.addEventListener('blur', () => {
        setTimeout(() => {
          if (!document.hasFocus()) {
            invoke('close_chat_window').catch(console.error);
          }
        }, 150);
      });
    }
  } catch (e) {
    console.error('[Chat] Failed to load auto_close config:', e);
    // Default to enabled on error
    window.addEventListener('blur', () => {
      setTimeout(() => {
        if (!document.hasFocus()) {
          invoke('close_chat_window').catch(console.error);
        }
      }, 150);
    });
  }
}

setupAutoClose();
inputEl.focus();
