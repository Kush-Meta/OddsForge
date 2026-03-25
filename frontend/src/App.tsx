import React from 'react';
import { BrowserRouter as Router, Routes, Route, Link, useLocation } from 'react-router-dom';
import { BarChart3, Target, Download, Users, Menu, X, Brain } from 'lucide-react';
import Dashboard from './pages/Dashboard';
import EdgeFinder from './pages/EdgeFinder';
import DatasetBuilder from './pages/DatasetBuilder';
import TeamProfile from './pages/TeamProfile';
import HowItWorks from './pages/HowItWorks';
import MLAnalytics from './pages/MLAnalytics';
import BouncingBall from './components/BouncingBall';
import './index.css';

const NAV_ITEMS = [
  { path: '/',              icon: BarChart3, label: 'Dashboard'      },
  { path: '/edges',         icon: Target,    label: 'Edge Finder'    },
  { path: '/ml',            icon: Brain,     label: 'ML Analytics'   },
  { path: '/dataset',       icon: Download,  label: 'Dataset Builder'},
  { path: '/teams',         icon: Users,     label: 'Teams'          },
];

const Navbar: React.FC = () => {
  const [open, setOpen] = React.useState(false);
  const location = useLocation();

  return (
    <nav className="navbar">
      <div className="nav-container">
        <Link to="/" className="nav-brand" onClick={() => setOpen(false)}>
          <div className="nav-logo-ring">🎯</div>
          <span>
            <span className="nav-brand-text">Odds</span>
            <span className="nav-brand-accent">Forge</span>
          </span>
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
              <Icon size={16} />
              <span>{label}</span>
            </Link>
          ))}
        </div>

        <div className="nav-status">
          <div className="nav-status-dot" />
          Live
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
          <Route path="/"          element={<Dashboard />}      />
          <Route path="/edges"     element={<EdgeFinder />}     />
          <Route path="/dataset"   element={<DatasetBuilder />} />
          <Route path="/teams"          element={<TeamProfile />}  />
          <Route path="/teams/:id"      element={<TeamProfile />}  />
          <Route path="/how-it-works"   element={<HowItWorks />}   />
          <Route path="/ml"             element={<MLAnalytics />}  />
        </Routes>
      </main>
      <BouncingBall />
    </div>
  </Router>
);

export default App;
