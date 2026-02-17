export const COMPANION_INSTALL_TASK_TITLE =
  'Install and integrate Wickeban Web Companion';

export const COMPANION_INSTALL_TASK_DESCRIPTION = `Goal: Install and integrate the wickeban-web-companion so it renders at the app root in development.

Do:
1) Detect package manager from lockfiles and use it:
   - pnpm-lock.yaml → pnpm add wickeban-web-companion
   - yarn.lock → yarn add wickeban-web-companion
   - package-lock.json → npm i wickeban-web-companion
   - bun.lockb → bun add wickeban-web-companion
   If already listed in package.json dependencies, skip install.

2) Detect framework and app entry:
   - Next.js (pages router): pages/_app.(tsx|js)
   - Next.js (app router): app/layout.(tsx|js) or an app/providers.(tsx|js)
   - Vite/CRA: src/main.(tsx|jsx|ts|js) and src/App.(tsx|jsx|ts|js)
   - Monorepo: operate in the correct package for the web app.
   Confirm by reading package.json and directory structure.

3) Integrate the component:
   import { WickebanWebCompanion } from 'wickeban-web-companion';
   - Vite/CRA: render <WickebanWebCompanion /> at the app root.
   - Next.js (pages): render in pages/_app.*
   - Next.js (app): render in app/layout.* or a client providers component.
   - For Next.js, if SSR issues arise, use dynamic import with ssr: false.

4) Verify:
   - Type-check, lint/format if configured.
   - Ensure it compiles and renders without SSR/hydration errors.

Acceptance:
- wickeban-web-companion is installed in the correct package.
- The component is rendered once at the app root without SSR/hydration errors.
- Build/type-check passes.`;
