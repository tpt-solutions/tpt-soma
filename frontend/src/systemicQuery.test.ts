import { describe, expect, it } from 'vitest';
import { CANONICAL_QUERY, buildSystemicRequest, validateQuery, type QueryForm } from './systemicQuery';

const withChanges = (changes: Partial<QueryForm>): QueryForm => ({ ...CANONICAL_QUERY, ...changes });

describe('buildSystemicRequest', () => {
  it('maps the canonical example to the snake_case request body', () => {
    const body = buildSystemicRequest(CANONICAL_QUERY);
    expect(body.target_node_id).toBe('BreastTissue');
    expect(body.adipose_secretion).toBe(0.8);
    expect(body.igf1_clearance).toBe(0.2);
    expect(body.igf1_growth).toBe(0.5);
    expect(body.steps).toBe(600);
    expect(body.edges).toEqual([
      { from: 'AdiposeTissue', to: 'IGF1', edge_type: 'cross_talk' },
      { from: 'IGF1', to: 'BreastTissue', edge_type: 'cross_talk' },
    ]);
  });

  it('rejects a dangling edge before sending', () => {
    const form = withChanges({ edges: [{ from: 'AdiposeTissue', to: 'Missing' }] });
    expect(() => buildSystemicRequest(form)).toThrow('Every edge must connect two existing nodes.');
  });

  it('rejects duplicate node ids', () => {
    const form = withChanges({
      nodes: [
        { id: 'x', kind: 'IGF1' },
        { id: 'x', kind: 'BreastTissue' },
      ],
    });
    expect(validateQuery(form)).toBe('Node ids must be unique.');
  });

  it('rejects a target that is not in the node list', () => {
    expect(validateQuery(withChanges({ targetNodeId: 'Liver' }))).toBe(
      'Pick a target node from the node list.',
    );
  });

  it('rejects non-positive dt and non-integer or zero steps', () => {
    expect(validateQuery(withChanges({ dt: 0 }))).toBe('dt must be a positive number.');
    expect(validateQuery(withChanges({ steps: 0 }))).toBe('steps must be a positive whole number.');
    expect(validateQuery(withChanges({ steps: 2.5 }))).toBe('steps must be a positive whole number.');
  });

  it('rejects parameters that are not finite numbers', () => {
    expect(validateQuery(withChanges({ igf1Growth: Number.NaN }))).toBe(
      'Every parameter must be a number.',
    );
  });

  it('accepts the canonical example', () => {
    expect(validateQuery(CANONICAL_QUERY)).toBeNull();
  });
});
