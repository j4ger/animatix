import { defineConfig } from 'astro/config';
export default defineConfig({ output: 'static', base: process.env.SITE_BASE || '/', trailingSlash: 'always', build: { format: 'directory' }, vite: { build: { sourcemap: false } } });
