import { defineConfig } from 'vite';

// PORT comes from the Amp orb service; allowedHosts lets the portal proxy through.
export default defineConfig({
  base: './',
  build: { target: 'es2022' },
  server: { host: '0.0.0.0', port: Number(process.env.PORT) || 5173, strictPort: true, allowedHosts: true },
});
