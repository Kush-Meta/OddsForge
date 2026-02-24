import React from 'react';
import { BrowserRouter as Router, Routes, Route, Link, useLocation } from 'react-router-dom';
import { BarChart3, Target, Download, Users, Menu, X } from 'lucide-react';
import Dashboard from './pages/Dashboard';
import EdgeFinder from './pages/EdgeFinder';
import DatasetBuilder from './pages/DatasetBuilder';
import TeamProfile from './pages/TeamProfile';
import './App.css';

const NAV_ITEMS = [
  { path: '/',       icon: BarChart3, label: 'Dashboard'      },
  { path: '/edges',  icon: Target,    label: 'Edge Finder'    },
  { path: '/dataset',icon: Download,  label: 'Dataset Builder'},
  { path: '/teams',  icon: Users,     label: 'Teams'          },
];

const Navbar: React.FC = () => {
  const [open, setOpen] = React.useState(false);
  const location = useLocation();

  return (
    <nav className="navbar">
      <div className="nav-container">
        <Link to="/" className="nav-brand" onClick={() => setOpen(false)}>
          <span className="nav-brand-icon" style={{ fontSize: '1.4rem' }}>🎯</span>
          <span className="nav-brand-text">OddsForge</span>
        </Link>

        <button className="nav-toggle" onClick={() => setOpen(o => !o)} aria-label="Toggle menu">
          {open ? <X size={22} /> : <Menu size={22} />}
        </button>

        <div className={`nav-menu ${open ? 'nav-menu-active' : ''}`}>
          {NAV_ITEMS.map(({ path, icon: Icon, label }) => (
            <Link
              key={path}
              to={path}
              className={`nav-link ${location.pathname === path ? 'nav-link-active' : ''}`}
              onClick={() => setOpen(false)}
            >
              <Icon size={18} />
              <span>{label}</span>
            </Link>
          ))}
        </div>
      </div>
    </nav>
  );
};

const App: React.FC = () => (
  <Router>
    <div className="app">
      <Navbar />
      <main className="main-content">
        <Routes>
          <Route path="/"          element={<Dashboard />}     />
          <Route path="/edges"     element={<EdgeFinder />}    />
          <Route path="/dataset"   element={<DatasetBuilder />}/>
          <Route path="/teams"     element={<TeamProfile />}   />
          <Route path="/teams/:id" element={<TeamProfile />}   />
        </Routes>
      </main>
    </div>
  </Router>
);

export default App;
