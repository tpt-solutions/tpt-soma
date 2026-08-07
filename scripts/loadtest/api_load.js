import http from 'k6/http';
import { check, sleep } from 'k6';
import { Rate } from 'k6/metrics';

const API = __ENV.TPT_API_URL || 'http://localhost:8080';
const TOKEN = __ENV.TPT_TOKEN || '';
const SAMPLE_ID = __ENV.TPT_SAMPLE_ID || '00000000-0000-0000-0000-000000000000';
const COHORT_ID = __ENV.TPT_COHORT_ID || 'cohort-a';

const failRate = new Rate('request_failures');

function authHeaders() {
  return {
    headers: {
      Authorization: `Bearer ${TOKEN}`,
      'Content-Type': 'application/json',
    },
  };
}

// Endpoints exercised (see crates/tpt-soma-api/src/server.rs route table).
const reads = [
  ['variants', `GET`, `/api/v1/variants/${SAMPLE_ID}`],
  ['expression', `GET`, `/api/v1/expression/${SAMPLE_ID}`],
  ['umap', `GET`, `/api/v1/umap/${SAMPLE_ID}`],
  ['clinical', `GET`, `/api/v1/clinical-observations/${SAMPLE_ID}`],
  ['cgm', `GET`, `/api/v1/cgm/${SAMPLE_ID}`],
  ['join', `POST`, `/api/v1/join/variant-expression`],
];

export const options = {
  scenarios: {
    ramp: {
      executor: 'ramping-vus',
      startVUs: 1,
      stages: [
        { duration: '1m', target: 25 },
        { duration: '3m', target: 25 },
        { duration: '1m', target: 0 },
      ],
    },
  },
  thresholds: {
    http_req_failed: ['rate<0.05'],
    http_req_duration: ['p(95)<1500'],
  },
};

export default function () {
  for (const [name, method, path] of reads) {
    let res;
    if (method === 'GET') {
      res = http.get(`${API}${path}`, authHeaders());
    } else {
      res = http.post(`${API}${path}`, JSON.stringify({ sample_id: SAMPLE_ID }), authHeaders());
    }
    const ok = check(res, {
      [`${name} 2xx`]: (r) => r.status >= 200 && r.status < 300,
    });
    failRate.add(!ok);
  }

  // DP aggregate export (requires an export-scoped token). Exercises the
  // epsilon-budget guard under concurrency.
  const agg = http.post(
    `${API}/api/v1/cohorts/${COHORT_ID}/aggregate/count`,
    JSON.stringify({ column: 'cell_id' }),
    authHeaders()
  );
  const aggOk = check(agg, {
    'aggregate 2xx or budget-4xx': (r) =>
      (r.status >= 200 && r.status < 300) || r.status === 429 || r.status === 403,
  });
  failRate.add(!aggOk);

  sleep(1);
}
