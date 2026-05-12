import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom'
import Layout from './components/Layout'
import Dashboard from './pages/Dashboard'
import FiscalJournal from './pages/FiscalJournal'
import ZReports from './pages/ZReports'

export default function App() {
  return (
    <BrowserRouter>
      <Layout>
        <Routes>
          <Route path="/" element={<Navigate to="/dashboard" replace />} />
          <Route path="/dashboard"      element={<Dashboard />} />
          <Route path="/fiscal-journal" element={<FiscalJournal />} />
          <Route path="/z-reports"      element={<ZReports />} />
        </Routes>
      </Layout>
    </BrowserRouter>
  )
}
