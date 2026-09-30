import assert from 'node:assert/strict';
import { chromium } from '../ui/node_modules/playwright/index.mjs';
import { mkdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
const artifacts = fileURLToPath(new URL('../artifacts/',import.meta.url));
await mkdir(artifacts,{recursive:true});
const browser = await chromium.launch({headless:true});
const context = await browser.newContext({viewport:{width:1440,height:1100}});
const page = await context.newPage();
const errors = [];
page.on('pageerror',error => errors.push(error.message));
async function settled() {
  await page.getByText('Live query results',{exact:true}).waitFor({timeout:30000});
  await page.waitForFunction(() => {
    const reset = document.querySelector('button.reset');
    return reset instanceof HTMLButtonElement && !reset.disabled;
  });
}
try {
  await page.goto('http://127.0.0.1:5421');
  if (process.argv.includes('--disconnected')) {
    await page.getByText('Disconnected / stale',{exact:true}).waitFor();
    await page.getByRole('heading',{name:'Awaiting current queries',exact:true}).waitFor();
    assert.equal(await page.getByRole('button',{name:'Reset scene',exact:true}).isDisabled(),true);
    assert.equal(await page.locator('[data-entity]').count(),0);
    assert.equal(await page.getByRole('heading',{name:'Nothing in the way',exact:true}).count(),0);
    await page.getByRole('alert').filter({hasText:'The Drasi server is unavailable'}).waitFor();
    await page.screenshot({path:`${artifacts}/disconnected.png`,fullPage:true});
    console.log('PASS: absent backend is visibly stale, server unavailability is shown, editing is disabled, and no scene/results are fabricated. This is NOT an end-to-end integration check.');
  } else {
  await settled();
  await page.getByRole('button',{name:'Reset scene',exact:true}).click();
  await settled();
  assert.equal(await page.title(),'Obstacle Impact | Drasi');
  assert.equal(await page.getByRole('heading',{level:1,name:'Obstacle Impact',exact:true}).count(),1);
  assert.doesNotMatch(await page.locator('body').innerText(), /\bmetres?\b|\bmeters?\b|\(m\)|\b\d+(?:\.\d+)? m\b/i);
  assert.equal(await page.locator('svg.scene .scene-label').textContent(),'SCENE');
  assert.equal(await page.getByRole('button',{name:'Information about this demo',exact:true}).getAttribute('aria-haspopup'),'dialog');
  const nameInput = page.getByLabel('Name',{exact:true});
  const originalName = await nameInput.inputValue();
  const originalRevision = await page.locator('.revision').textContent();
  await nameInput.fill('Unsaved overlay draft');
  await page.getByRole('button',{name:'Information about this demo',exact:true}).click();
  await page.getByRole('dialog',{name:'Architecture Overview',exact:true}).waitFor();
  assert.equal(page.url(),'http://127.0.0.1:5421/');
  await page.getByRole('button',{name:'Close architecture overview',exact:true}).click();
  await page.locator('#architecture-overlay').waitFor({state:'detached'});
  assert.equal(await nameInput.inputValue(),'Unsaved overlay draft','Closing info must preserve unsaved scene edits');
  assert.equal(await page.locator('.revision').textContent(),originalRevision,'Info must not submit a command or reset the scene');
  await nameInput.fill(originalName);
  const footer = page.locator('footer');
  assert.deepEqual(await footer.getByRole('link').allTextContents(),['Actual graph API ↗','Drasi Server Web UI ↗']);
  assert.equal(await footer.locator('span').count(),0);
  assert.equal(await footer.getByRole('link',{name:'Actual graph API ↗',exact:true}).getAttribute('href'),'/api/v1/instances/move-a-wall/computation');
  const [admin] = await Promise.all([
    context.waitForEvent('page'),
    footer.getByRole('link',{name:'Drasi Server Web UI ↗',exact:true}).click(),
  ]);
  admin.on('pageerror',error => errors.push(error.message));
  await admin.waitForLoadState('domcontentloaded');
  assert.equal(admin.url(),'http://127.0.0.1:8421/ui/');
  assert.equal(await admin.title(),'Drasi Server');
  await admin.getByText('move-a-wall',{exact:true}).first().waitFor({timeout:30000});
  await admin.locator('.react-flow__node').filter({hasText:/geometry/}).first().waitFor({timeout:30000});
  await admin.close();
  await page.bringToFront();
  await page.evaluate(() => window.scrollTo(0,0));
  assert.equal(await page.locator('.brand,.topbar,.intro').count(),0);
  const inspector = page.getByRole('region',{name:'Follow the change',exact:true});
  const flow = inspector.getByRole('navigation',{name:'Computation graph',exact:true});
  assert.equal(await flow.count(),1);
  assert.equal(await page.locator('main > .pipeline,.query-tabs').count(),0);
  assert.doesNotMatch(await page.locator('body').innerText(), /\bfloor\b/i);
  await page.getByText('Drag an obstacle across a planned path.',{exact:true}).waitFor();
  const editor = page.getByRole('region',{name:'Edit scene',exact:true});
  const selection = editor.getByRole('combobox',{name:'Selected object',exact:true});
  await selection.selectOption('wall');
  const wall = page.locator('[data-entity="wall"]');
  const position = await wall.boundingBox();
  const scene = await page.locator('svg.scene').boundingBox();
  assert.ok(position && scene);
  assert.ok(scene.y < 240,`The scene should start near the top, not below tall headers (y=${scene.y})`);
  const legend = await page.getByRole('group',{name:'Scene legend',exact:true}).boundingBox();
  assert.ok(legend && legend.y + legend.height <= scene.y && legend.x > scene.x + scene.width/2,'The legend must be at the top right, above the scene');
  assert.equal(await page.locator('.scene-note,.journeys,.journey-card').count(),0);
  assert.deepEqual((await page.locator('svg.scene .cart-name').allTextContents()).sort(),['Ada','Grace']);
  const gracePath = page.locator('svg.scene .path').filter({hasText:'Bring assembly parts'});
  const gracePoints = (await gracePath.locator('.centerline').getAttribute('points')).split(' ').map(point => point.split(',').map(Number));
  assert.equal(gracePoints.length,25,'Grace uses the sampled arc from the source fixture');
  assert.deepEqual(gracePoints[0],[2,14]);
  assert.deepEqual(gracePoints[12],[11.5,9.5]);
  assert.deepEqual(gracePoints.at(-1),[21,14]);
  const inspectorPosition = await inspector.boundingBox();
  assert.ok(inspectorPosition && inspectorPosition.y < 850,`Removing the duplicate scene content should bring the inspector up (y=${inspectorPosition?.y})`);
  const scenePosition = await page.locator('.scene-panel').boundingBox();
  const initialImpact = await page.locator('.impact-panel').boundingBox();
  assert.ok(scenePosition && initialImpact);
  assert.equal(inspectorPosition.x,scenePosition.x);
  assert.equal(inspectorPosition.width,scenePosition.width,'The inspector must match the scene panel width');
  assert.ok(Math.abs(inspectorPosition.y - (scenePosition.y + scenePosition.height + 20)) < 1,'The inspector must sit directly below the scene panel');
  const editorPosition = await editor.boundingBox();
  assert.ok(editorPosition && editorPosition.y < scene.y + scene.height/2,'Object editing must be beside the scene, not beneath it');
  await page.locator('svg.scene .path').filter({hasText:'Deliver packaging'}).click();
  assert.equal(await selection.inputValue(),'journey-ada','Journeys remain selectable in the scene without separate cards');
  await selection.selectOption('wall');
  const x = position.x + position.width / 2, y = position.y + position.height / 2;
  await page.mouse.move(x,y);
  await page.mouse.down();
  await page.mouse.move(x,y-scene.height*2.5/18,{steps:12});
  await page.mouse.up();
  await page.getByRole('heading',{name:'1 affected journey',exact:true}).waitFor();
  await settled();
  const expandedImpact = await page.locator('.impact-panel').boundingBox();
  const stableInspector = await inspector.boundingBox();
  assert.ok(expandedImpact && stableInspector);
  assert.ok(expandedImpact.height > initialImpact.height,'The impact panel must actually expand during this check');
  assert.equal(stableInspector.y,inspectorPosition.y,'An expanding impact panel must not push the inspector down');
  for (const [label,id] of [
    ['Source','scene-inputs'],['Context query','geometry-context'],['Geometry transformer','obstructions'],
    ['Impact query','affected-journeys'],['SSE / UI','geometry-status'],
  ]) {
    const stage = flow.getByRole('button',{name:`${label} ${id}`,exact:true});
    await stage.click();
    assert.equal(await stage.getAttribute('aria-pressed'),'true');
    assert.equal(await flow.locator('button[aria-pressed="true"]').count(),1);
    assert.equal(await stage.getAttribute('aria-controls'),'query-records');
    const response = await page.request.get(`http://127.0.0.1:5421/api/v1/instances/move-a-wall/queries/${id}/results`);
    assert.equal(response.ok(),true);
    const body = await response.json();
    assert.equal(body.success,true);
    const expected = id === 'scene-inputs' || id === 'geometry-context'
      ? body.data.flatMap(row => row.objects.map(payload => JSON.parse(payload))) : body.data;
    for (const row of expected) {
      assert.equal(Object.hasOwn(row,'floor'),false,`${id} must not return the removed field`);
      if (row.type === 'entity') assert.equal(Object.hasOwn(row.entity,'floor'),false);
    }
    assert.deepEqual(JSON.parse(await inspector.getByTestId('query-records').textContent()),expected,`${label} must display its actual query records`);
  }
  await flow.getByRole('button',{name:'Source scene-inputs',exact:true}).focus();
  await page.keyboard.press('Tab');
  await page.keyboard.press('Enter');
  assert.equal(await flow.getByRole('button',{name:'Context query geometry-context',exact:true}).getAttribute('aria-pressed'),'true');
  await flow.getByRole('button',{name:'Impact query affected-journeys',exact:true}).click();
  await page.screenshot({path:`${artifacts}/obstructed.png`,fullPage:true});
  await page.reload();
  await page.getByRole('heading',{name:'1 affected journey',exact:true}).waitFor();
  await settled();
  await page.getByRole('button',{name:'Reconnect',exact:true}).click();
  await settled();
  await page.getByRole('heading',{name:'1 affected journey',exact:true}).waitFor();
  await context.setOffline(true);
  await page.getByText('Disconnected / stale',{exact:true}).waitFor();
  await context.setOffline(false);
  await settled();
  await page.getByLabel('Coordinates',{exact:true}).fill('[[10,7],[13,7],[13,8],[10,8]]');
  await page.getByRole('button',{name:'Apply input change',exact:true}).click();
  await page.getByRole('heading',{name:'Nothing in the way',exact:true}).waitFor();
  await settled();
  await wall.focus();
  await page.keyboard.press('ArrowUp');
  await settled();
  assert.deepEqual(JSON.parse(await page.getByLabel('Coordinates',{exact:true}).inputValue())[0],[10,6.75]);
  await page.getByLabel('Coordinates',{exact:true}).fill('[[0,0],[2,2],[2,0],[0,2]]');
  await page.getByRole('button',{name:'Apply input change',exact:true}).click();
  await page.getByRole('alert').filter({hasText:'only simple convex polygons'}).waitFor();
  await page.getByRole('button',{name:'Reset scene',exact:true}).click();
  await settled();
  await selection.selectOption('journey-ada');
  await page.getByLabel('Coordinates',{exact:true}).fill('[[2,6.5],[21,6.5]]');
  await page.getByRole('button',{name:'Apply input change',exact:true}).click();
  await page.getByRole('heading',{name:'1 affected journey',exact:true}).waitFor();
  await settled();
  await page.getByRole('button',{name:'Remove object',exact:true}).click();
  await page.getByRole('heading',{name:'Nothing in the way',exact:true}).waitFor();
  await settled();
  await page.getByRole('button',{name:'Reset scene',exact:true}).click();
  await settled();
  for (const kind of ['cart','journey','obstacle','destination']) {
    await page.getByRole('button',{name:`+ ${kind}`,exact:true}).click();
    await page.getByLabel('Name',{exact:true}).fill(`Temporary ${kind}`);
    await page.getByRole('button',{name:'Add object',exact:true}).click();
    await settled();
    await page.getByRole('heading',{name:`Temporary ${kind}`,exact:true}).waitFor();
    await page.getByLabel('Name',{exact:true}).fill(`Renamed ${kind}`);
    await page.getByRole('button',{name:'Apply input change',exact:true}).click();
    await settled();
    await page.getByRole('heading',{name:`Renamed ${kind}`,exact:true}).waitFor();
    await page.getByRole('button',{name:'Remove object',exact:true}).click();
    await settled();
    assert.equal(await selection.getByRole('option',{name:new RegExp(`Renamed ${kind}`)}).count(),0);
  }
  const revision = await page.locator('.revision').textContent();
  await page.getByRole('button',{name:'+ obstacle',exact:true}).click();
  await page.getByLabel('Name',{exact:true}).fill('Invalid draft');
  await page.getByLabel('Coordinates',{exact:true}).fill('[[0,0],[2,2],[2,0],[0,2]]');
  await page.getByRole('button',{name:'Add object',exact:true}).click();
  await page.getByRole('alert').filter({hasText:'only simple convex polygons'}).waitFor();
  assert.equal(await page.getByLabel('Name',{exact:true}).inputValue(),'Invalid draft');
  await page.getByRole('button',{name:'Cancel',exact:true}).click();
  assert.equal(await page.locator('.revision').textContent(),revision,'A rejected/cancelled draft must not change the source');
  assert.equal(await page.locator('footer').getByRole('status').count(),0);
  await page.getByRole('button',{name:'Dismiss',exact:true}).click();
  await page.locator('[data-entity="wall"]').click();
  await page.screenshot({path:`${artifacts}/clear.png`,fullPage:true});
  for (const width of [768,390]) {
    await page.setViewportSize({width,height:1000});
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth),true,`No horizontal overflow at ${width}px`);
    await selection.selectOption('journey-ada');
    await editor.getByRole('heading',{name:'Deliver packaging',exact:true}).waitFor();
    await flow.getByRole('button',{name:'SSE / UI geometry-status',exact:true}).click();
    assert.equal(await flow.getByRole('button',{name:'SSE / UI geometry-status',exact:true}).getAttribute('aria-pressed'),'true');
  }
  await page.screenshot({path:`${artifacts}/mobile.png`,fullPage:true});
  console.log('PASS: independently sized scene/inspector column, integrated flow inspector with real query records and keyboard selection, compact scene UI, adjacent object editor, responsive layout, real drag/retraction and path-only query impacts, reload, offline/reconnect, keyboard command, visible validation errors, CRUD for all four entity kinds, cancel/rejected draft preservation, reset. Screenshots in artifacts/.');
  }
  assert.deepEqual(errors,[]);
} finally { await browser.close(); }
