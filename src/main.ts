import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import lottie from 'lottie-web';
import animationData from './cat.json';

const mascot = document.getElementById('mascot')!;
const settingsBtn = document.getElementById('settings-btn')!;

const anim = lottie.loadAnimation({
  container: document.getElementById('lottie')!,
  renderer: 'svg',
  loop: true,
  autoplay: true,
  animationData,
});

function clearStateClasses() {
  const classes = ['idle', 'walk', 'peek', 'disappear', 'reappear', 'interact-wave', 'interact-jump', 'interact-spin'];
  for (const c of classes) {
    mascot.classList.remove(c);
  }
}

listen<{ state: string; peek_edge?: string; interact_type?: string }>('mascot:state', (event) => {
  const { state, interact_type } = event.payload;
  clearStateClasses();

  if (state === 'interact' && interact_type) {
    mascot.classList.add(`interact-${interact_type}`);
  } else {
    mascot.classList.add(state);
  }

  if (state === 'walk') {
    anim.setSpeed(1.5);
  } else if (state === 'disappear') {
    anim.setSpeed(0.5);
  } else if (state === 'interact') {
    anim.setSpeed(2);
  } else {
    anim.setSpeed(1);
  }
});

settingsBtn.addEventListener('click', () => {
  invoke('open_settings_window').catch(console.error);
});
