import React from 'react';
import { BrowserRouter as Router, Routes, Route, Link, useLocation } from 'react-router-dom';
import { BarChart3, TrendingUp, Download, Users, Target, Menu, X } from 'lucide-react';
import Dashboard from './pages/Dashboard';
import EdgeFinder from './pages/EdgeFinder';
import DatasetBuilder from './pages/DatasetBuilder';
import TeamProfile from './pages/TeamProfile';
import './App.css';

const Navbar: React.FC = () => {
  const [isOpen, setIsOpen] = React.useState(false);
  const location = useLocation();

  const navItems = [
    { path: '/', icon: BarChart3, label: 'Dashboard' },
    { path: '/edges', icon: Target, label: 'Edge Finder' },
    { path: '/dataset', icon: Download, label: 'Dataset Builder' },
    { path: '/teams', icon: Users, label: 'Teams' },
  ];

  const isActive = (path: string) => location.pathname === path;

  return (
    <nav className="navbar">
      <div className="nav-container">
        <Link to="/" className="nav-brand">
          <TrendingUp className="nav-brand-icon" />
          <span className="nav-brand-text">OddsForge</span>
        </Link>

        <button 
          className="nav-toggle"
          onClick={() => setIsOpen(!isOpen)}
        >
          {isOpen ? <X size={24} /> : <Menu size={24} />}
        </button>

        <div className={`nav-menu ${isOpen ? 'nav-menu-active' : ''}`}>
          {navItems.map((item) => {
            const Icon = item.icon;
            return (
              <Link
                key={item.path}
                to={item.path}
                className={`nav-link ${isActive(item.path) ? 'nav-link-active' : ''}`}
                onClick={() => setIsOpen(false)}
              >
                <Icon size={20} />
                <span>{item.label}</span>
              </Link>
            );
          })}
        </div>
      </div>
    </nav>
  );
};

const App: React.FC = () => {
  return (
    <Router>
      <div className="app">
        <Navbar />
        <main className="main-content">
          <Routes>
            <Route path="/" element={<Dashboard />} />
            <Route path="/edges" element={<EdgeFinder />} />
            <Route path="/dataset" element={<DatasetBuilder />} />
            <Route path="/teams" element={<TeamProfile />} />
            <Route path="/teams/:teamId" element={<TeamProfile />} />
          </Routes>
        </main>
      </div>
    </Router>
  );
};

export default App;