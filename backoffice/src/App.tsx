import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom'
import { AuthProvider } from './context/AuthContext'
import { SiteProvider } from './context/SiteContext'
import ProtectedRoute from './components/ProtectedRoute'
import Layout from './components/Layout'
import LoginPage from './pages/LoginPage'
import Dashboard from './pages/Dashboard'
import FiscalJournal from './pages/FiscalJournal'
import ZReports from './pages/ZReports'
import MenuManager from './pages/MenuManager'
import CategoryManager from './pages/CategoryManager'
import ModifierGroupManager from './pages/ModifierGroupManager'
import ProductList from './pages/ProductList'
import ProductForm from './pages/ProductForm'
import ComboList from './pages/ComboList'
import ComboForm from './pages/ComboForm'
import GroupList from './pages/GroupList'
import GroupForm from './pages/GroupForm'
import PromotionList from './pages/PromotionList'
import PromotionForm from './pages/PromotionForm'

export default function App() {
  return (
    <BrowserRouter>
      <AuthProvider>
        <SiteProvider>
          <Routes>
            <Route path="/login" element={<LoginPage />} />
            <Route element={<ProtectedRoute />}>
              <Route element={<Layout />}>
                <Route path="/" element={<Navigate to="/dashboard" replace />} />
                <Route path="/dashboard"      element={<Dashboard />} />
                <Route path="/fiscal-journal" element={<FiscalJournal />} />
                <Route path="/z-reports"      element={<ZReports />} />
                <Route path="/categories"     element={<CategoryManager />} />
                <Route path="/products"       element={<ProductList />} />
                <Route path="/products/new"   element={<ProductForm />} />
                <Route path="/products/:id"   element={<ProductForm />} />
                <Route path="/combos"         element={<ComboList />} />
                <Route path="/combos/new"     element={<ComboForm />} />
                <Route path="/combos/:id"     element={<ComboForm />} />
                <Route path="/modifiers"      element={<ModifierGroupManager />} />
                <Route path="/menu"           element={<MenuManager />} />
                <Route path="/promotions"     element={<PromotionList />} />
                <Route path="/promotions/new" element={<PromotionForm />} />
                <Route path="/promotions/:id" element={<PromotionForm />} />
                <Route path="/groups"         element={<GroupList />} />
                <Route path="/groups/new"     element={<GroupForm />} />
                <Route path="/groups/:id"     element={<GroupForm />} />
              </Route>
            </Route>
          </Routes>
        </SiteProvider>
      </AuthProvider>
    </BrowserRouter>
  )
}
