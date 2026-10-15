import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
export default defineConfig({plugins:[react()],clearScreen:false,server:{host:'127.0.0.1',port:5173,strictPort:true},test:{environment:'jsdom',include:['src/**/*.test.{ts,tsx}']},build:{target:'es2022',chunkSizeWarningLimit:1500}});
