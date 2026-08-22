import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import "./assets/main.css"
import { persistPlugin } from "./stores/persistPlugin";

const app = createApp(App);
const pinia = createPinia();
pinia.use(persistPlugin);
app.use(pinia);
app.mount("#app");
