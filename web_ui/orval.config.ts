import { defineConfig } from 'orval';

export default defineConfig({
  backend: {
    input: 'openapi.json',
    output: {
      target: 'src/api/generated/client.ts',
      schemas: 'src/api/generated/model',
      client: 'fetch',
      mode: 'tags-split',
      baseUrl: 'http://localhost:8991',
      clean: true,
      prettier: false,
    },
  },
});
