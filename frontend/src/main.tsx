import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { createBrowserRouter, RouterProvider, type RouteObject } from 'react-router'
import './index.css'
import App from './App.tsx'
import { ThemeProvider } from './lib/theme/ThemeProvider.tsx'
import { Toaster } from './components/ui/sonner.tsx'

const routes: RouteObject[] = [{ path: '/', element: <App /> }]

if (import.meta.env.DEV) {
  const { designRoutes } = await import('./routes/design')
  routes.push(...designRoutes)
}

const router = createBrowserRouter(routes)

const rootElement = document.getElementById('root')
if (!rootElement) {
  throw new Error('Reverie: #root element not found in document. Check index.html.')
}
createRoot(rootElement).render(
  <StrictMode>
    <ThemeProvider>
      <RouterProvider router={router} />
      <Toaster />
    </ThemeProvider>
  </StrictMode>,
)
