import { mount } from 'svelte';
import './app.css';
import App from './App.svelte';

const target = document.getElementById('app')!;
// Le serveur y a mis une version lisible sans JavaScript : l'app la remplace.
target.replaceChildren();

export default mount(App, { target });
