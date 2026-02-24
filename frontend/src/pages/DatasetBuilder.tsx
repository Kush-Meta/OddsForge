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

const DatasetBuilder: React.FC = () => {
  const [request, setRequest] = useState<DatasetRequest>({
    sport: 'football',
    stats_categories: ['basic'],
    format: 'csv',
  });
  
  const [loading, setLoading] = useState(false);
  const [result, setResult] = useState<any>(null);
  const [error, setError] = useState<string | null>(null);

  const sportOptions = [
    { value: 'football', label: 'Football (Soccer)' },
    { value: 'basketball', label: 'Basketball (NBA)' },
  ];

  const categoryOptions = [
    { 
      value: 'basic', 
      label: 'Basic Match Data',
      description: 'Match results, scores, dates, teams'
    },
    { 
      value: 'teams', 
      label: 'Team Information',
      description: 'ELO ratings, league info'
    },
    { 
      value: 'predictions', 
      label: 'Model Predictions',
      description: 'Win probabilities, confidence scores'
    },
  ];

  const formatOptions = [
    { value: 'csv', label: 'CSV', description: 'Comma-separated values for Excel/analysis' },
    { value: 'json', label: 'JSON', description: 'Structured data for programming' },
  ];

  const handleInputChange = (field: keyof DatasetRequest, value: any) => {
    setRequest(prev => ({ ...prev, [field]: value }));
    setResult(null);
    setError(null);
  };

  const handleCategoryChange = (category: string, checked: boolean) => {
    setRequest(prev => ({
      ...prev,
      stats_categories: checked
        ? [...prev.stats_categories, category]
        : prev.stats_categories.filter(c => c !== category)
    }));
  };

  const generateDataset = async () => {
    if (request.stats_categories.length === 0) {
      setError('Please select at least one data category');
      return;
    }

    setLoading(true);
    setError(null);
    setResult(null);

    try {
      const response = await apiService.generateDataset(request);
      setResult(response);
    } catch (err) {
      setError('Failed to generate dataset. Make sure the backend is running and has data.');
      console.error('Error generating dataset:', err);
    } finally {
      setLoading(false);
    }
  };

  const downloadDataset = () => {
    if (result?.download_url) {
      // In a real app, this would be a full URL to download the file
      window.open(`http://localhost:3000${result.download_url}`, '_blank');
    }
  };

  return (
    <div className="page">
      <div className="page-header">
        <div className="page-title">
          <Download size={32} />
          <div>
            <h1>Dataset Builder</h1>
            <p>Create custom datasets for analysis and research</p>
          </div>
        </div>
      </div>

      <div className="dataset-builder">
        <div className="builder-form">
          <div className="form-section">
            <div className="section-header">
              <Settings size={20} />
              <h3>Dataset Configuration</h3>
            </div>

            <div className="form-group">
              <label htmlFor="sport">Sport</label>
              <select
                id="sport"
                value={request.sport}
                onChange={(e) => handleInputChange('sport', e.target.value)}
                className="form-select"
              >
                {sportOptions.map(option => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
            </div>

            <div className="form-group">
              <label htmlFor="format">Export Format</label>
              <div className="format-options">
                {formatOptions.map(option => (
                  <label key={option.value} className="radio-option">
                    <input
                      type="radio"
                      name="format"
                      value={option.value}
                      checked={request.format === option.value}
                      onChange={(e) => handleInputChange('format', e.target.value)}
                    />
                    <div className="radio-content">
                      <span className="radio-label">{option.label}</span>
                      <span className="radio-description">{option.description}</span>
                    </div>
                  </label>
                ))}
              </div>
            </div>
          </div>

          <div className="form-section">
            <div className="section-header">
              <Calendar size={20} />
              <h3>Date Range (Optional)</h3>
            </div>

            <div className="form-row">
              <div className="form-group">
                <label htmlFor="date_from">From Date</label>
                <input
                  type="date"
                  id="date_from"
                  value={request.date_from || ''}
                  onChange={(e) => handleInputChange('date_from', e.target.value || undefined)}
                  className="form-input"
                />
              </div>
              <div className="form-group">
                <label htmlFor="date_to">To Date</label>
                <input
                  type="date"
                  id="date_to"
                  value={request.date_to || ''}
                  onChange={(e) => handleInputChange('date_to', e.target.value || undefined)}
                  className="form-input"
                />
              </div>
            </div>
          </div>

          <div className="form-section">
            <div className="section-header">
              <FileText size={20} />
              <h3>Data Categories</h3>
            </div>

            <div className="category-options">
              {categoryOptions.map(option => (
                <label key={option.value} className="checkbox-option">
                  <input
                    type="checkbox"
                    checked={request.stats_categories.includes(option.value)}
                    onChange={(e) => handleCategoryChange(option.value, e.target.checked)}
                  />
                  <div className="checkbox-content">
                    <span className="checkbox-label">{option.label}</span>
                    <span className="checkbox-description">{option.description}</span>
                  </div>
                </label>
              ))}
            </div>
          </div>

          <div className="form-actions">
            <button 
              onClick={generateDataset}
              disabled={loading || request.stats_categories.length === 0}
              className="generate-btn"
            >
              {loading ? (
                <>
                  <div className="spinner" />
                  Generating...
                </>
              ) : (
                <>
                  <Download size={18} />
                  Generate Dataset
                </>
              )}
            </button>
          </div>
        </div>

        <div className="builder-preview">
          <h3>Preview & Results</h3>
          
          <div className="preview-section">
            <h4>Dataset Summary</h4>
            <div className="summary-grid">
              <div className="summary-item">
                <span className="summary-label">Sport:</span>
                <span className="summary-value">{request.sport}</span>
              </div>
              <div className="summary-item">
                <span className="summary-label">Format:</span>
                <span className="summary-value">{request.format.toUpperCase()}</span>
              </div>
              <div className="summary-item">
                <span className="summary-label">Categories:</span>
                <span className="summary-value">{request.stats_categories.length}</span>
              </div>
              <div className="summary-item">
                <span className="summary-label">Date Range:</span>
                <span className="summary-value">
                  {request.date_from || request.date_to 
                    ? `${request.date_from || 'All'} to ${request.date_to || 'All'}`
                    : 'All Time'
                  }
                </span>
              </div>
            </div>
          </div>

          {error && (
            <div className="result-section error">
              <AlertCircle size={20} />
              <div>
                <h4>Generation Failed</h4>
                <p>{error}</p>
              </div>
            </div>
          )}

          {result && (
            <div className="result-section success">
              <CheckCircle size={20} />
              <div>
                <h4>Dataset Generated Successfully!</h4>
                <div className="result-details">
                  <p><strong>Rows:</strong> {result.rows}</p>
                  <p><strong>Format:</strong> {result.format.toUpperCase()}</p>
                  <p><strong>Generated:</strong> {new Date(result.generated_at).toLocaleString()}</p>
                </div>
                <button onClick={downloadDataset} className="download-btn">
                  <Download size={18} />
                  Download Dataset
                </button>
              </div>
            </div>
          )}

          <div className="preview-section">
            <h4>Sample Columns</h4>
            <div className="sample-columns">
              {request.stats_categories.includes('basic') && (
                <div className="column-group">
                  <span className="group-title">Basic:</span>
                  <span className="columns">match_date, home_team, away_team, home_score, away_score</span>
                </div>
              )}
              {request.stats_categories.includes('teams') && (
                <div className="column-group">
                  <span className="group-title">Teams:</span>
                  <span className="columns">home_elo, away_elo, league, sport</span>
                </div>
              )}
              {request.stats_categories.includes('predictions') && (
                <div className="column-group">
                  <span className="group-title">Predictions:</span>
                  <span className="columns">home_win_prob, away_win_prob, draw_prob, confidence</span>
                </div>
              )}
            </div>
          </div>
        </div>
      </div>

      <div className="usage-tips">
        <h3>Usage Tips</h3>
        <ul>
          <li><strong>CSV format</strong> is great for Excel analysis and data science tools like pandas</li>
          <li><strong>JSON format</strong> is perfect for web applications and API integrations</li>
          <li>Use date ranges to focus on specific seasons or time periods</li>
          <li>Combine multiple categories for comprehensive analysis datasets</li>
          <li>Large datasets may take a moment to generate - please be patient</li>
        </ul>
      </div>
    </div>
  );
};

export default DatasetBuilder;