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
    assert.equal(await page.getByRole('button',{name:'Reset floor',exact:true}).isDisabled(),true);
    assert.equal(await page.locator('[data-entity]').count(),0);
    assert.equal(await page.getByRole('heading',{name:'Nothing in the way',exact:true}).count(),0);
    await page.getByRole('alert').filter({hasText:'The Drasi server is unavailable'}).waitFor();
    await page.screenshot({path:`${artifacts}/disconnected.png`,fullPage:true});
    console.log('PASS: absent backend is visibly stale, server unavailability is shown, editing is disabled, and no scene/results are fabricated. This is NOT an end-to-end integration check.');
  } else {
  await settled();
  await page.getByRole('button',{name:'Reset floor',exact:true}).click();
  await settled();
  const wall = page.locator('[data-entity="wall"]');
  const position = await wall.boundingBox();
  const scene = await page.locator('svg.scene').boundingBox();
  assert.ok(position && scene);
  const x = position.x + position.width / 2, y = position.y + position.height / 2;
  await page.mouse.move(x,y);
  await page.mouse.down();
  await page.mouse.move(x,y-scene.height*2.5/18,{steps:12});
  await page.mouse.up();
  await page.getByRole('heading',{name:'1 affected journey',exact:true}).waitFor();
  await settled();
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
  await page.getByRole('button',{name:'Reset floor',exact:true}).click();
  await settled();
  await page.locator('.entity-list').getByRole('button',{name:/Deliver packaging/}).click();
  await page.getByLabel('Coordinates',{exact:true}).fill('[[2,6.5],[21,6.5]]');
  await page.getByRole('button',{name:'Apply input change',exact:true}).click();
  await page.getByRole('heading',{name:'1 affected journey',exact:true}).waitFor();
  await settled();
  await page.getByRole('button',{name:'Remove object',exact:true}).click();
  await page.getByRole('heading',{name:'Nothing in the way',exact:true}).waitFor();
  await settled();
  await page.getByRole('button',{name:'Reset floor',exact:true}).click();
  await settled();
  for (const kind of ['cart','journey','obstacle','destination']) {
    await page.getByRole('button',{name:`+ ${kind}`,exact:true}).click();
    await page.getByLabel('Name',{exact:true}).fill(`Temporary ${kind}`);
    await page.getByLabel('Floor',{exact:true}).last().fill('upper');
    await page.getByRole('button',{name:'Add object',exact:true}).click();
    await settled();
    await page.getByRole('heading',{name:`Temporary ${kind}`,exact:true}).waitFor();
    await page.getByLabel('Name',{exact:true}).fill(`Renamed ${kind}`);
    await page.getByRole('button',{name:'Apply input change',exact:true}).click();
    await settled();
    await page.getByRole('heading',{name:`Renamed ${kind}`,exact:true}).waitFor();
    await page.getByRole('button',{name:'Remove object',exact:true}).click();
    await settled();
    assert.equal(await page.locator('.entity-list').getByRole('button',{name:new RegExp(`Renamed ${kind}`)}).count(),0);
  }
  await page.locator('[data-entity="wall"]').click();
  await page.screenshot({path:`${artifacts}/clear.png`,fullPage:true});
  console.log('PASS: real drag/retraction and path-only query impacts, reload, offline/reconnect, keyboard command, visible validation error, CRUD for all four entity kinds, reset. Screenshots in artifacts/.');
  }
  assert.deepEqual(errors,[]);
} finally { await browser.close(); }
