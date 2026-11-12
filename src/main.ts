import { mount } from 'svelte'
import './app.css'
import App from './App.svelte'

const app = mount(App, { target: document.getElementById('app')! })

// Mounting is synchronous, so by the time this line runs there is a real window behind it and the drawn
// shape from `index.html` has served its purpose. If mounting throws, this does not run and the skeleton
// stays up — which is the picture worth having.
document.getElementById('boot')?.remove()

export default app
