import '@fontsource-variable/inter';
import '@fontsource-variable/jetbrains-mono';
import './lib/styles/tokens.css';
import './lib/styles/base.css';
import { mount } from 'svelte';
import App from './App.svelte';
import { selectApi } from './lib/api';
import { installContextMenuGuard } from './lib/util/contextMenu';
import { AppStore } from './lib/stores/app.svelte';

installContextMenuGuard();

const app = new AppStore(selectApi());

mount(App, { target: document.getElementById('app') as HTMLElement, props: { app } });
