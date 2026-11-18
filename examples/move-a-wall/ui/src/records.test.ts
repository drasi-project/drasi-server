import { describe, expect, it } from 'vitest';
import { graphRow, graphRowKey } from '@drasi/react/client';
import { entity, sceneEntity, status, translate, validate } from './records';
describe('query-backed scene records',() => {
  const wall = entity({id:'wall',name:'Wall',active:true,shape:{kind:'obstacle',vertices:[[1,1],[2,1],[2,2],[1,2]]}});
  it('uses only scene entity fields in query records and commands',() => {
    expect(Object.keys(wall).sort()).toEqual(['active','id','name','shape']);
    const row = {id:'wall',kind:'obstacle',name:'Wall',active:true,vertices:[[1,1],[2,1],[2,2],[1,2]],
      cart_id:null,destination_id:null,points:null,point:null,radius:null,clearance:null};
    expect(sceneEntity(row)).toEqual(wall);
    expect(Object.keys(translate(wall,.5,0)).sort()).toEqual(['active','id','name','shape']);
  });
  it('keys rows by canonical element identity, not by a row signature or domain field',() => {
    const identity = (id: string) => ({namespace:'obstructions/out',id});
    const first = graphRow(identity('00000000000000ff'),{id:'journey/wall'});
    const same = graphRow(identity('00000000000000ff'),{id:'journey/wall'});
    const other = graphRow(identity('0000000000000100'),{id:'journey/wall'});
    expect(graphRowKey(first)).toBe(graphRowKey(same));
    expect(graphRowKey(first)).not.toBe(graphRowKey(other));
    expect(() => graphRowKey({id:'journey/wall',row_signature:18446744073709551615})).toThrow();
  });
  it('rejects malformed query data instead of rendering clear state',() => {
    expect(() => sceneEntity({id:'wall',kind:'obstacle',name:'Wall',active:true,vertices:null})).toThrow();
    expect(() => validate('scene-status',{id:'current',revision:1,command:'reset',objects:0})).toThrow();
    expect(() => validate('journey-footprints',{journey_id:'j',cart_id:'c',points:[[0,0]],radius:'1',clearance:0})).toThrow();
    expect(() => validate('affected-journeys',{id:'j/w'})).toThrow();
    expect(() => entity({...wall,shape:{kind:'circle',radius:1}})).toThrow();
  });
  it('translates only tentative input coordinates without deriving geometry',() => {
    const changed = translate(wall,.5,-.25);
    expect(changed.shape).toEqual({kind:'obstacle',vertices:[[1.5,.75],[2.5,.75],[2.5,1.75],[1.5,1.75]]});
    expect(wall.shape).toEqual({kind:'obstacle',vertices:[[1,1],[2,1],[2,2],[1,2]]});
  });
  it('parses explicit source revisions, including empty scenes',() => {
    expect(status({id:'current',revision:2,command:'clear',changed:['wall'],objects:0}))
      .toEqual({revision:2,command:'clear',changed:['wall'],objects:0});
  });
});
