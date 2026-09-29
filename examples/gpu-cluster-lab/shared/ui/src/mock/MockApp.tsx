import { useRef, useState } from 'react';
import { Lab, type Command, type View, type Views } from '../App';
import type { QueryId } from '../rows';
import type { MockRows } from './fixtures';
import { applyCommand } from './model';
import { scenarios, snapshot } from './scenarios';
import { label } from '../labels';

export function mockViews(rows: MockRows, scene: string, retry: () => void): Views {
  const view = (id: QueryId): View => ({
    data: rows[id], loading: scene === 'bootstrap', stale: scene === 'feed-stale',
    error: scene === 'query-error' && id === 'ui-policy' ? new Error('Example failure: policy updates are unavailable') : null,
    status: scene === 'feed-stale' ? 'stale mock snapshot' : scene === 'bootstrap' ? 'loading mock snapshot' : 'mock snapshot', retry,
  });
  return {
    'ui-gpus': view('ui-gpus'), 'ui-workloads': view('ui-workloads'), 'ui-placements': view('ui-placements'),
    'ui-resilience': view('ui-resilience'), 'ui-decisions': view('ui-decisions'), 'ui-status': view('ui-status'),
    'ui-timeline': view('ui-timeline'), 'ui-clusters': view('ui-clusters'), 'ui-policy': view('ui-policy'),
  };
}

export default function MockApp({ initialSnapshot = 'healthy' }: { initialSnapshot?: string }) {
  const [scene, setScene] = useState(initialSnapshot);
  const [rows, setRows] = useState(() => snapshot(initialSnapshot));
  const current = useRef(rows);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const send: Command = async (path, method, body, _key, expectedRevision) => {
    setError(null); setNotice(null);
    try {
      const next = applyCommand(current.current, path, method, body, expectedRevision);
      current.current = next;
      setRows(next);
      if (path.startsWith('/api/demo/presets/')) setScene(String(next['ui-status'][0].scenario) === 'baseline' ? 'healthy'
        : next['ui-status'][0].scenario === 'fragmentation' ? 'fragmented' : 'regional');
      setNotice('Preview updated in this tab only. No backend request was sent. Reload an example state to restore its prepared results.');
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
      throw failure;
    }
  };
  const retry = () => setNotice('This is a prepared example of disconnected updates. Select another example state; no connection is attempted.');
  const load = (id: string) => {
    const next = snapshot(id);
    current.current = next; setRows(next); setScene(id); setError(null); setNotice(null);
  };
  return <Lab key={scene} connection={{ initialized: scene !== 'bootstrap', error: scene === 'feed-stale' ? new Error('Example failure: live updates are disconnected') : null, retry }}
    command={{ send, pending: false, notice: null, error }} views={mockViews(rows, scene, retry)}
    demoPanel={<section className="demo-panel" aria-labelledby="demo-panel-title">
      <div className="demo-panel-heading">
        <div className="demo-panel-title"><h2 id="demo-panel-title">Demo preview</h2><span className="mode-label">MOCK DATA</span></div>
        <div className="scenario-controls" role="group" aria-label="Demo setup"><div className="snapshot-picker" aria-label="Example demo states">
          <label htmlFor="snapshot">Example state</label>
          <select id="snapshot" value={scene} title={scenarios.find(s => s.id === scene)?.title} onChange={e => load(e.target.value)}>
            {['baseline', 'fragmentation', 'regional-boundary'].map(name => <optgroup key={name} label={label('scenario', name)}>
              {scenarios.filter(s => s.fixture === name).map(s => <option key={s.id} value={s.id}>{s.title}</option>)}
            </optgroup>)}
          </select><button aria-label="Reload example state" onClick={() => load(scene)}>Reload</button>
        </div></div>
      </div>
      <p>Prepared example data, not live results. Time is paused. Editing settings changes only this tab and does not run the backend.
        Selecting an example state replaces local edits. No real GPUs or model downloads.</p>
      {notice && <p className="demo-notice" role="status">{notice}</p>}
    </section>}/>;
}
