import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { createBrowserRouter, RouterProvider } from 'react-router'
import './index.css'
import App from './App.tsx'
import ExploreIndex from './pages/design/explore/_shared/index.tsx'
import MidnightGold from './pages/design/explore/midnight-gold.tsx'
import Signal from './pages/design/explore/signal.tsx'
import AtelierInk from './pages/design/explore/atelier-ink.tsx'

const router = createBrowserRouter([
  { path: '/', element: <App /> },
  { path: '/design/explore', element: <ExploreIndex /> },
  { path: '/design/explore/midnight-gold', element: <MidnightGold /> },
  { path: '/design/explore/signal', element: <Signal /> },
  { path: '/design/explore/atelier-ink', element: <AtelierInk /> },
])

const rootElement = document.getElementById('root')
if (!rootElement) {
  throw new Error('Reverie: #root element not found in document. Check index.html.')
}
createRoot(rootElement).render(
  <StrictMode>
    <RouterProvider router={router} />
  </StrictMode>,
)
