import { invoke } from '@tauri-apps/api/core';

const messagesEl = document.getElementById('messages')!;
const inputEl = document.getElementById('chat-input') as HTMLTextAreaElement;
const sendBtn = document.getElementById('send-btn')! as HTMLButtonElement;
const closeBtn = document.getElementById('close-btn')!;
const welcomeState = document.getElementById('welcome-state')!;

let isLoading = false;
let userScrolledUp = false;

/* ── Markdown Parser ── */

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

function parseMarkdown(text: string): string {
  let html = escapeHtml(text);

  // Code blocks
  html = html.replace(/```(\w*)\n?([\s\S]*?)```/g, (_, lang, code) => {
    const language = lang || 'text';
    const cleanCode = code.replace(/\n$/, '');
    return `<pre><div class="code-block-header"><span class="code-lang">${language}</span><button class="code-copy-btn" onclick="copyCode(this)"><svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path></svg> 复制</button></div><code>${cleanCode}</code></pre>`;
  });

  // Inline code
  html = html.replace(/`([^`]+)`/g, '<code>$1</code>');

  // Bold
  html = html.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');

  // Italic
  html = html.replace(/\*([^*]+)\*/g, '<em>$1</em>');

  // Links
  html = html.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" target="_blank">$1</a>');

  // Blockquote
  html = html.replace(/^&gt;\s*(.+)$/gm, '<blockquote>$1</blockquote>');

  // Unordered lists
  html = html.replace(/^\-\s*(.+)$/gm, '<li>$1</li>');
  html = html.replace(/(<li>.*<\/li>\n?)+/g, '<ul>$&</ul>');
  html = html.replace(/<ul>(<li>.*<\/li>\n?)+<\/ul>/g, (match) => match.replace(/<\/li>\n<li>/g, '</li><li>'));

  // Paragraphs
  const paragraphs = html.split(/\n\n+/);
  html = paragraphs
    .map((p) => {
      const trimmed = p.trim();
      if (!trimmed) return '';
      if (trimmed.startsWith('<')) return trimmed;
      return `<p>${trimmed.replace(/\n/g, '<br>')}</p>`;
    })
    .join('\n');

  return html;
}

/* ── Copy Functions ── */

function copyToClipboard(text: string): Promise<void> {
  if ((navigator as any).clipboard?.writeText) {
    return (navigator as any).clipboard.writeText(text);
  }
  const textarea = document.createElement('textarea');
  textarea.value = text;
  textarea.style.position = 'fixed';
  textarea.style.opacity = '0';
  document.body.appendChild(textarea);
  textarea.select();
  document.execCommand('copy');
  document.body.removeChild(textarea);
  return Promise.resolve();
}

(window as any).copyCode = function (btn: HTMLButtonElement) {
  const pre = btn.closest('pre');
  if (!pre) return;
  const code = pre.querySelector('code');
  if (!code) return;
  const text = code.textContent || '';
  copyToClipboard(text).then(() => {
    btn.classList.add('copied');
    btn.innerHTML = '✓ 已复制';
    setTimeout(() => {
      btn.classList.remove('copied');
      btn.innerHTML = '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path></svg> 复制';
    }, 2000);
  });
};

function copyMessage(btn: HTMLButtonElement, text: string) {
  copyToClipboard(text).then(() => {
    btn.classList.add('copied');
    const original = btn.innerHTML;
    btn.innerHTML = '✓ 已复制';
    setTimeout(() => {
      btn.classList.remove('copied');
      btn.innerHTML = original;
    }, 2000);
  });
}

/* ── Message Rendering ── */

const COLLAPSE_THRESHOLD = 300; // chars
const COLLAPSE_LINES = 6;

function formatUserText(text: string): string {
  return escapeHtml(text).replace(/\n/g, '<br>');
}

function shouldCollapse(text: string): boolean {
  return text.length > COLLAPSE_THRESHOLD || (text.match(/\n/g) || []).length >= COLLAPSE_LINES;
}

function createMessageRow(role: 'user' | 'ai' | 'error', content: string): HTMLElement {
  const row = document.createElement('div');
  row.className = `message-row ${role}`;

  const contentWrapper = document.createElement('div');
  contentWrapper.className = 'message-content';

  const bubble = document.createElement('div');
  bubble.className = 'message-bubble';

  if (role === 'error') {
    bubble.textContent = content;
  } else if (role === 'user') {
    const needsCollapse = shouldCollapse(content);
    const textDiv = document.createElement('div');
    textDiv.className = 'message-text';
    textDiv.innerHTML = formatUserText(content);

    bubble.appendChild(textDiv);

    if (needsCollapse) {
      textDiv.classList.add('collapsed');
      const toggleBtn = document.createElement('button');
      toggleBtn.className = 'collapse-toggle';
      toggleBtn.textContent = '展开';
      toggleBtn.onclick = () => {
        const isCollapsed = textDiv.classList.contains('collapsed');
        textDiv.classList.toggle('collapsed', !isCollapsed);
        toggleBtn.textContent = isCollapsed ? '收起' : '展开';
      };
      bubble.appendChild(toggleBtn);
    }
  } else {
    bubble.innerHTML = parseMarkdown(content);
  }

  const meta = document.createElement('div');
  meta.className = 'message-meta';

  const time = document.createElement('span');
  time.textContent = new Date().toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' });

  const actions = document.createElement('div');
  actions.className = 'message-actions';

  if (role !== 'error') {
    const copyBtn = document.createElement('button');
    copyBtn.className = 'action-btn';
    copyBtn.innerHTML = '复制';
    copyBtn.onclick = () => copyMessage(copyBtn, content);
    actions.appendChild(copyBtn);
  }

  meta.appendChild(time);
  meta.appendChild(actions);

  contentWrapper.appendChild(bubble);
  contentWrapper.appendChild(meta);

  row.appendChild(contentWrapper);

  return row;
}

let typingIndicator: HTMLElement | null = null;

function showTypingIndicator() {
  if (typingIndicator) return;

  typingIndicator = document.createElement('div');
  typingIndicator.className = 'typing-indicator';
  typingIndicator.innerHTML = `
    <div class="typing-dots"><span></span><span></span><span></span></div>
  `;

  messagesEl.appendChild(typingIndicator);
  scrollToBottom();
}

function hideTypingIndicator() {
  if (typingIndicator) {
    typingIndicator.remove();
    typingIndicator = null;
  }
}

function appendMessage(role: 'user' | 'ai' | 'error', content: string) {
  if (welcomeState && !welcomeState.classList.contains('hidden')) {
    welcomeState.classList.add('hidden');
  }

  const row = createMessageRow(role, content);
  messagesEl.appendChild(row);
  scrollToBottom();
}

function scrollToBottom() {
  if (!userScrolledUp) {
    messagesEl.scrollTop = messagesEl.scrollHeight;
  }
}

/* ── Input Handling ── */

function autoResize() {
  inputEl.style.height = 'auto';
  inputEl.style.height = Math.min(inputEl.scrollHeight, 120) + 'px';
}

inputEl.addEventListener('input', autoResize);

inputEl.addEventListener('keydown', (e) => {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault();
    sendMessage();
  }
});

function setLoading(loading: boolean) {
  isLoading = loading;
  sendBtn.disabled = loading;

  if (!loading) {
    inputEl.disabled = false;
    inputEl.focus();
  } else {
    inputEl.disabled = true;
  }
}

async function sendMessage() {
  const text = inputEl.value.trim();
  if (!text || isLoading) return;

  appendMessage('user', text);
  inputEl.value = '';
  inputEl.style.height = 'auto';
  setLoading(true);
  showTypingIndicator();

  try {
    const reply = await invoke<string>('chat_send', { message: text });
    hideTypingIndicator();
    appendMessage('ai', reply);
  } catch (e: any) {
    hideTypingIndicator();
    appendMessage('error', String(e));
  } finally {
    setLoading(false);
  }
}

sendBtn.addEventListener('click', sendMessage);

closeBtn.addEventListener('click', () => {
  invoke('close_chat_window').catch(console.error);
});

/* ── Chat History ── */

interface ChatMessage {
  role: string;
  content: string;
  timestamp: number;
}

async function loadChatHistory() {
  try {
    const history = await invoke<ChatMessage[]>('get_chat_history');
    if (history.length > 0) {
      welcomeState.classList.add('hidden');
      for (const msg of history) {
        const row = createMessageRow(msg.role as 'user' | 'ai' | 'error', msg.content);
        messagesEl.appendChild(row);
      }
      scrollToBottom();
    }
  } catch (e) {
    console.error('[Chat] Failed to load history:', e);
  }
}

const clearBtn = document.getElementById('clear-btn');
if (clearBtn) {
  clearBtn.addEventListener('click', async () => {
    try {
      await invoke('clear_chat_history');
      messagesEl.innerHTML = '';
      messagesEl.appendChild(welcomeState);
      welcomeState.classList.remove('hidden');
    } catch (e) {
      console.error('[Chat] Failed to clear history:', e);
    }
  });
}

/* ── Scroll Detection ── */

messagesEl.addEventListener('scroll', () => {
  const nearBottom = messagesEl.scrollHeight - messagesEl.scrollTop - messagesEl.clientHeight < 50;
  userScrolledUp = !nearBottom;
});

/* ── Auto-close on blur ── */

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
loadChatHistory();
inputEl.focus();
