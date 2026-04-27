import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { createBrowserRouter, RouterProvider } from 'react-router'
import './index.css'
import App from './App.tsx'

const router = createBrowserRouter([{ path: '/', element: <App /> }])

const rootElement = document.getElementById('root')
if (!rootElement) {
  throw new Error('Reverie: #root element not found in document. Check index.html.')
}
createRoot(rootElement).render(
  <StrictMode>
    <RouterProvider router={router} />
  </StrictMode>,
)
