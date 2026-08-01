import React, { useState } from 'react';

function App() {
  const [cohort, setCohort] = useState('');
  return (
    <div style={{ padding: 24 }}>
      <h1>tpt-soma</h1>
      <p>Multi-scale computational physiology platform</p>
      <input
        placeholder="Cohort ID"
        value={cohort}
        onChange={(e) => setCohort(e.target.value)}
      />
      <p>Phase 0 scaffold — query UI and UMAP viewer coming in Phase 1.</p>
    </div>
  );
}

export default App;
