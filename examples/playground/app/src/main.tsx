import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App'
import { SourceDataProvider } from './contexts/SourceDataContext'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <SourceDataProvider>
      <App />
    </SourceDataProvider>
  </StrictMode>,
)
