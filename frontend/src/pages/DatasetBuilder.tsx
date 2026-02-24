import React, { useState } from 'react';
import { Download, Settings, Calendar, FileText, AlertCircle, CheckCircle } from 'lucide-react';
import { apiService } from '../services/api';

interface DatasetRequest {
  sport: string;
  teams?: string[];
  date_from?: string;
  date_to?: string;
  stats_categories: string[];
  format: string;
}

interface DatasetResult {
  download_url?: string;
  format: string;
  rows: number;
  generated_at: string;
}

const SPORT_OPTIONS = [
  { value: 'football',   label: '⚽ Football (EPL)' },
  { value: 'basketball', label: '🏀 Basketball (NBA)' },
];

const CATEGORY_OPTIONS = [
  { value: 'basic',       label: 'Basic Match Data',    description: 'Match results, scores, dates, venues' },
  { value: 'teams',       label: 'Team Information',    description: 'ELO ratings, league, sport' },
  { value: 'predictions', label: 'Model Predictions',   description: 'Win probabilities, draw chances, confidence scores' },
];

const FORMAT_OPTIONS = [
  { value: 'csv',  label: 'CSV',  description: 'Comma-separated — works in Excel, pandas, R' },
  { value: 'json', label: 'JSON', description: 'Structured — ideal for APIs and web apps' },
];

const DatasetBuilder: React.FC = () => {
  const [request, setRequest] = useState<DatasetRequest>({
    sport: 'football',
    stats_categories: ['basic'],
    format: 'csv',
  });
  const [loading, setLoading] = useState(false);
  const [result, setResult]   = useState<DatasetResult | null>(null);
  const [error, setError]     = useState<string | null>(null);

  const set = (field: keyof DatasetRequest, value: unknown) => {
    setRequest(prev => ({ ...prev, [field]: value }));
    setResult(null);
    setError(null);
  };

  const toggleCategory = (cat: string, checked: boolean) => {
    set('stats_categories',
      checked
        ? [...request.stats_categories, cat]
        : request.stats_categories.filter(c => c !== cat)
    );
  };

  const generate = async () => {
    if (request.stats_categories.length === 0) {
      setError('Select at least one data category.');
      return;
    }
    setLoading(true);
    setError(null);
    setResult(null);
    try {
      const payload = {
        ...request,
        date_from: request.date_from
          ? new Date(request.date_from).toISOString()
          : undefined,
        date_to: request.date_to
          ? new Date(request.date_to).toISOString()
          : undefined,
      };
      const res = await apiService.generateDataset(payload);
      setResult(res);
    } catch {
      setError('Failed to generate dataset. Make sure the backend is running and has data.');
    } finally {
      setLoading(false);
    }
  };

  const download = () => {
    if (result?.download_url) {
      const base = process.env.REACT_APP_API_URL || 'http://localhost:3000';
      window.open(`${base}${result.download_url}`, '_blank');
    }
  };

  const sampleCols: Record<string, string[]> = {
    basic:       ['match_date', 'home_team', 'away_team', 'home_score', 'away_score', 'status'],
    teams:       ['home_elo', 'away_elo', 'league', 'sport'],
    predictions: ['home_win_prob', 'away_win_prob', 'draw_prob', 'confidence', 'model_version'],
  };

  return (
    <div className="page">
      <div className="page-header">
        <div className="page-title">
          <Download size={32} />
          <div>
            <h1>Dataset Builder</h1>
            <p>Export custom match &amp; prediction datasets for ML / analysis</p>
          </div>
        </div>
      </div>

      <div className="dataset-builder">
        {/* ── Form ── */}
        <div className="builder-form">
          {/* Sport */}
          <div className="form-section">
            <div className="section-header">
              <Settings size={18} />
              <h3>Configuration</h3>
            </div>

            <div className="form-group">
              <label htmlFor="sport">Sport</label>
              <select
                id="sport"
                className="form-select"
                value={request.sport}
                onChange={e => set('sport', e.target.value)}
              >
                {SPORT_OPTIONS.map(o => (
                  <option key={o.value} value={o.value}>{o.label}</option>
                ))}
              </select>
            </div>

            <div className="form-group">
              <label>Export Format</label>
              <div className="format-options">
                {FORMAT_OPTIONS.map(o => (
                  <label key={o.value} className="radio-option">
                    <input
                      type="radio"
                      name="format"
                      value={o.value}
                      checked={request.format === o.value}
                      onChange={e => set('format', e.target.value)}
                    />
                    <div className="radio-content">
                      <span className="radio-label">{o.label}</span>
                      <span className="radio-description">{o.description}</span>
                    </div>
                  </label>
                ))}
              </div>
            </div>
          </div>

          {/* Date range */}
          <div className="form-section">
            <div className="section-header">
              <Calendar size={18} />
              <h3>Date Range <span style={{ fontWeight: 400, color: 'var(--text-muted)' }}>(optional)</span></h3>
            </div>
            <div className="form-row">
              <div className="form-group">
                <label htmlFor="date_from">From</label>
                <input
                  type="date"
                  id="date_from"
                  className="form-input"
                  value={request.date_from || ''}
                  onChange={e => set('date_from', e.target.value || undefined)}
                />
              </div>
              <div className="form-group">
                <label htmlFor="date_to">To</label>
                <input
                  type="date"
                  id="date_to"
                  className="form-input"
                  value={request.date_to || ''}
                  onChange={e => set('date_to', e.target.value || undefined)}
                />
              </div>
            </div>
          </div>

          {/* Categories */}
          <div className="form-section">
            <div className="section-header">
              <FileText size={18} />
              <h3>Data Categories</h3>
            </div>
            <div className="category-options">
              {CATEGORY_OPTIONS.map(o => (
                <label key={o.value} className="checkbox-option">
                  <input
                    type="checkbox"
                    checked={request.stats_categories.includes(o.value)}
                    onChange={e => toggleCategory(o.value, e.target.checked)}
                  />
                  <div className="checkbox-content">
                    <span className="checkbox-label">{o.label}</span>
                    <span className="checkbox-description">{o.description}</span>
                  </div>
                </label>
              ))}
            </div>
          </div>

          <div className="form-actions">
            <button
              className="generate-btn"
              onClick={generate}
              disabled={loading || request.stats_categories.length === 0}
            >
              {loading
                ? <><div className="spinner" /> Generating…</>
                : <><Download size={18} /> Generate Dataset</>
              }
            </button>
          </div>
        </div>

        {/* ── Preview / Result ── */}
        <div className="builder-preview">
          <h3>Preview &amp; Results</h3>

          {/* Summary */}
          <div className="preview-section">
            <h4>Dataset Summary</h4>
            <div className="summary-grid">
              <div className="summary-item">
                <span className="summary-label">Sport</span>
                <span className="summary-value">{request.sport === 'football' ? '⚽ EPL' : '🏀 NBA'}</span>
              </div>
              <div className="summary-item">
                <span className="summary-label">Format</span>
                <span className="summary-value">{request.format.toUpperCase()}</span>
              </div>
              <div className="summary-item">
                <span className="summary-label">Categories</span>
                <span className="summary-value">{request.stats_categories.length} selected</span>
              </div>
              <div className="summary-item">
                <span className="summary-label">Date Range</span>
                <span className="summary-value">
                  {request.date_from || request.date_to
                    ? `${request.date_from || '∞'} → ${request.date_to || '∞'}`
                    : 'All time'}
                </span>
              </div>
            </div>
          </div>

          {/* Error */}
          {error && (
            <div className="result-section error">
              <AlertCircle size={20} />
              <div>
                <h4>Generation Failed</h4>
                <p>{error}</p>
              </div>
            </div>
          )}

          {/* Success */}
          {result && (
            <div className="result-section success">
              <CheckCircle size={20} />
              <div>
                <h4>Dataset Ready!</h4>
                <div className="result-details">
                  <p><strong>Rows:</strong> {result.rows}</p>
                  <p><strong>Format:</strong> {result.format.toUpperCase()}</p>
                  <p><strong>Generated:</strong> {new Date(result.generated_at).toLocaleString()}</p>
                </div>
                {result.download_url && (
                  <button className="download-btn" onClick={download}>
                    <Download size={16} />
                    Download {result.format.toUpperCase()}
                  </button>
                )}
              </div>
            </div>
          )}

          {/* Sample columns */}
          <div className="preview-section">
            <h4>Sample Columns</h4>
            <div className="sample-columns">
              {request.stats_categories.map(cat => (
                <div key={cat} className="column-group">
                  <span className="group-title">{cat.charAt(0).toUpperCase() + cat.slice(1)}:</span>
                  <span className="columns"> {sampleCols[cat]?.join(', ')}</span>
                </div>
              ))}
              {request.stats_categories.length === 0 && (
                <span style={{ color: 'var(--text-muted)', fontSize: '0.85rem' }}>
                  Select at least one category to see columns
                </span>
              )}
            </div>
          </div>
        </div>
      </div>

      {/* Tips */}
      <div className="usage-tips">
        <h3>Usage Tips</h3>
        <ul>
          <li><strong>CSV</strong> works directly in Excel, pandas, R, and Google Sheets</li>
          <li><strong>JSON</strong> is ideal for Python dicts, JavaScript, and REST APIs</li>
          <li>Select <em>Model Predictions</em> to include ELO-based win probabilities</li>
          <li>Use date ranges to export season slices or rolling windows for backtesting</li>
          <li>Combine <em>Teams</em> + <em>Predictions</em> for feature-rich ML training sets</li>
        </ul>
      </div>
    </div>
  );
};

export default DatasetBuilder;
