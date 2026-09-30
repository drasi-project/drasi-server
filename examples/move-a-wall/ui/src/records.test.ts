import { describe, expect, it } from 'vitest';
import { entity, inputs, key, translate, validate } from './records';
describe('query-backed scene records',() => {
  const wall = entity({id:'wall',name:'Wall',active:true,shape:{kind:'obstacle',vertices:[[1,1],[2,1],[2,2],[1,2]]}});
  it('uses only scene entity fields in query records and commands',() => {
    expect(Object.keys(wall).sort()).toEqual(['active','id','name','shape']);
    expect(inputs({objects:[JSON.stringify({type:'entity',revision:1,entity:wall})]}))
      .toEqual([{type:'entity',revision:1,entity:wall}]);
    expect(Object.keys(translate(wall,.5,0)).sort()).toEqual(['active','id','name','shape']);
  });
  it('uses stable domain IDs rather than unsafe numeric row signatures',() => {
    expect(key('obstructions',{id:'journey/wall',row_signature:18446744073709551615})).toBe('journey/wall');
    expect(key('geometry-context',{objects:[]})).toBe('geometry-context');
  });
  it('rejects malformed query data instead of rendering clear state',() => {
    expect(() => inputs({objects:['not JSON']})).toThrow();
    expect(() => validate('affected-journeys',{id:'j/w'})).toThrow();
    expect(() => entity({...wall,shape:{kind:'circle',radius:1}})).toThrow();
  });
  it('translates only tentative input coordinates without deriving geometry',() => {
    const changed = translate(wall,.5,-.25);
    expect(changed.shape).toEqual({kind:'obstacle',vertices:[[1.5,.75],[2.5,.75],[2.5,1.75],[1.5,1.75]]});
    expect(wall.shape).toEqual({kind:'obstacle',vertices:[[1,1],[2,1],[2,2],[1,2]]});
  });
  it('parses explicit source revisions, including empty scenes',() => {
    expect(inputs({objects:[JSON.stringify({type:'clock',revision:2,members:{},changed:['wall'],command:'clear'})]}))
      .toEqual([{type:'clock',revision:2,members:{},changed:['wall'],command:'clear'}]);
  });
});
