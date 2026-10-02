import type { ResultRow } from '@drasi/react/client';
import type { Views } from './App';
import { currentFeeds } from './Evidence';
import { label } from './labels';
import { recordMap, strings, text } from './rows';

export function dataPolicyState(views: Views, stale = false) {
  const status = views['ui-status'].data?.[0];
  return {
    rules: status ? recordMap(status, 'policy_rules', 'policy_id') : null,
    profiles: status ? recordMap(status, 'data_profiles', 'data_profile_id') : null,
    current: !stale && currentFeeds(views) && status?.policy_rules_current === true,
  };
}

export function dataProfile(views: Views, workload: ResultRow): ResultRow | undefined {
  return dataPolicyState(views).profiles?.find(profile => profile.data_profile_id === workload.data_profile_id);
}

export function ruleWorkloads(ruleId: string, views: Views): ResultRow[] {
  const profiles = dataPolicyState(views).profiles;
  return (views['ui-workloads'].data ?? []).filter(workload => profiles
    ? profiles.some(profile => profile.data_profile_id === workload.data_profile_id && profile.policy_id === ruleId)
    : views['ui-policy'].data?.some(pair => pair.workload_id === workload.workload_id
      && pair.data_profile_id === workload.data_profile_id && pair.policy_id === ruleId));
}

export function editorRules(views: Views): ResultRow[] {
  const state = dataPolicyState(views);
  if (state.rules) return state.rules.map(rule => ({ ...rule, policy_revision: rule.revision }));
  return views['ui-policy'].data?.filter(pair => pair.policy_revision !== null) ?? [];
}

export function RuleCriteria({ rule }: { rule: ResultRow }) {
  return <dl className="shared-rule-criteria">
    <dt>Customer must match</dt><dd>{text(rule, 'customer_id')}</dd>
    <dt>Classification must be</dt><dd>{strings(rule, 'allowed_classifications').map(value => label('classification', value)).join(', ') || 'None'}</dd>
    <dt>Purpose must be</dt><dd>{strings(rule, 'allowed_purposes').map(value => label('purpose', value)).join(', ') || 'None'}</dd>
    <dt>Destination must be in</dt><dd>{strings(rule, 'allowed_regions').map(value => label('region', value)).join(', ') || 'None'}</dd>
  </dl>;
}

export function DataPolicyPanel({ views, stale, disabled, onEdit }: {
  views: Views; stale: boolean; disabled: boolean; onEdit: (rule: ResultRow | null) => void;
}) {
  const state = dataPolicyState(views, stale);
  const observed = [...new Map((views['ui-policy'].data ?? []).filter(row => row.policy_revision !== null)
    .map(row => [text(row, 'policy_id'), row])).values()];
  return <div className="data-policy-content">
      <button type="button" className="edit-data-policy" disabled={disabled || !(state.rules?.length || observed.length)} onClick={() => onEdit(null)}>Edit data policy</button>
      <p><strong>Shared rules govern all workloads.</strong> A workload supplies data classification, purpose and a candidate destination;
        Drasi evaluates the applicable conditions and returns allowed, blocked or unknown. Workloads do not choose their rules.</p>
      <p className="muted">The conditions shown for each group of workloads must all pass.
        Permission makes a destination eligible; GPU capacity and replica separation still determine whether an allocation fits.</p>
      {state.rules && state.profiles ? <>
        {!state.current && <p className="status-text warning">Last received rule settings; a matching current evaluation is not available yet.</p>}
        <div className="shared-rule-list">{state.rules.map(rule => {
          const scopes = state.profiles!.filter(profile => profile.policy_id === rule.policy_id);
          const workloads = ruleWorkloads(text(rule, 'policy_id'), views);
          return <article className="shared-rule" key={text(rule, 'policy_id')}>
            <RuleCriteria rule={rule}/>
            <p><strong>Outcome:</strong> allow only when all conditions match; otherwise block. An evaluation error or missing input leaves permission unknown.</p>
            <p className="rule-scope">Workloads in this scope: {workloads.map(workload => text(workload, 'name')).join(', ') || 'None currently configured'}.</p>
            {state.rules!.length > 1 && <button type="button" disabled={disabled} onClick={() => onEdit({ ...rule, policy_revision: rule.revision })}>Edit these region rules</button>}
            <details className="technical-details"><summary>Rule identity and source</summary>
              <p>Data profiles select the rule scope: {scopes.length ? scopes.map(profile => label('data', text(profile, 'data_profile_id'))).join(', ') : 'No data profiles assigned'}.</p>
              <p>{text(rule, 'name')} · <code>{text(rule, 'policy_id')}</code> · revision {text(rule, 'revision')}</p>
              <p>Source: {text(rule, 'authority_ref')}</p>
            </details>
          </article>;
        })}</div>
        {!state.rules.length && <p>No shared rules are configured.</p>}
      </> : <>
        <p className="status-text warning" role="status">Full data-policy criteria are unavailable in this runtime. Showing observed regional settings only;
          customer, classification and permitted-purpose criteria require the updated query projection.</p>
        {observed.map(rule => <p key={text(rule, 'policy_id')}>For {ruleWorkloads(text(rule, 'policy_id'), views).map(workload => text(workload, 'name')).join(', ') || 'no currently observed workloads'}:{' '}
          configured regions {strings(rule, 'allowed_regions').map(region => label('region', region)).join(', ') || 'none'}.</p>)}
      </>}
  </div>;
}
